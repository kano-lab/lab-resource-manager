//! 空き状況メッセージ
//!
//! `/free`の応答を組み立てます。
//!
//! # 構成
//!
//! 「いま使えるか」に見出しで答え、続けてGPUの埋まり具合を記号の表で示し、
//! 最後に使用中の予約者と終了時刻を並べます。表は「どこが」空いているかを、
//! 内訳は「誰が・いつまで」を担い、同じことを二度書きません。
//!
//! # モジュール
//!
//! - `grid`: GPUの埋まり具合を表す記号の表

pub mod grid;

use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::common::EmailAddress;
use crate::domain::services::resource_usage::availability::{
    AvailabilityState, ResourceAvailability,
};
use crate::infrastructure::config::ResourceStyle;
use crate::infrastructure::notifier::formatter::{format_resources_styled, to_zoned};
use chrono::{DateTime, Utc};
use slack_morphism::prelude::*;
use std::collections::HashMap;

/// 空き状況メッセージを組み立てる
///
/// # 引数
/// * `availabilities` - 各リソースの空き（設定順）
/// * `now` - 状態を判定する時刻
/// * `timezone` - 時刻表示に使うタイムゾーン名。未指定ならボットのローカル時刻
/// * `owner_displays` - 予約者の表示名。解決できなかった予約者はメールアドレスで表示する
pub fn build(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    timezone: Option<&str>,
    owner_displays: &HashMap<EmailAddress, String>,
) -> SlackMessageContent {
    if availabilities.is_empty() {
        return SlackMessageContent::new()
            .with_text("予約できるリソースが設定されていません。".to_string());
    }

    let headline = headline(availabilities, now);
    let mut blocks: Vec<SlackBlock> = vec![section(format!("*{}*", headline))];

    if let Some(line) = earliest_free_line(availabilities, now, timezone) {
        blocks.push(section(line));
    }

    if let Some(rendered) = grid::render(&servers_of(availabilities, now)) {
        blocks.push(section(format!("```\n{}\n```", rendered)));
        blocks.push(context(format!(
            "`{}` 空き 　`{}` 使用中",
            grid::FREE_MARK,
            grid::BUSY_MARK
        )));
    }

    if let Some(rooms) = room_lines(availabilities, now, timezone, owner_displays) {
        blocks.push(section(rooms));
    }

    if let Some(ongoing) = ongoing_gpu_lines(availabilities, now, timezone, owner_displays) {
        blocks.push(SlackBlock::Divider(SlackDividerBlock::new()));
        blocks.push(section(ongoing));
    }

    blocks.push(context(format!(
        "{} 時点",
        to_zoned(now, timezone).format("%-m月%-d日 %H:%M")
    )));

    SlackMessageContent::new()
        .with_text(headline)
        .with_blocks(blocks)
}

/// 空いている台数を伝える見出し
///
/// GPUと部屋は数え方が違う（台と件）ため、種類ごとに数えて並べる。
/// 空きのない種類は句ごと省く。
fn headline(availabilities: &[ResourceAvailability], now: DateTime<Utc>) -> String {
    let free_gpus = count_free(availabilities, now, |resource| {
        matches!(resource, Resource::Gpu(_))
    });
    let free_rooms = count_free(availabilities, now, |resource| {
        matches!(resource, Resource::Room { .. })
    });

    let mut parts = Vec::new();
    if free_gpus > 0 {
        parts.push(format!("GPU {}台", free_gpus));
    }
    if free_rooms > 0 {
        parts.push(format!("部屋 {}件", free_rooms));
    }

    if parts.is_empty() {
        return "いま空いているリソースはありません".to_string();
    }

    format!("いま {} が空いています", parts.join("、"))
}

/// 条件に合うリソースのうち、いま空いているものを数える
fn count_free(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    matches_kind: impl Fn(&Resource) -> bool,
) -> usize {
    availabilities
        .iter()
        .filter(|availability| matches_kind(availability.resource()))
        .filter(|availability| is_free_at(availability, now))
        .count()
}

/// 指定時刻に空いているか
fn is_free_at(availability: &ResourceAvailability, now: DateTime<Utc>) -> bool {
    matches!(availability.state_at(now), AvailabilityState::Free { .. })
}

/// ひとつも空いていないとき、最も早く空くものを伝える
///
/// 空きがあるときは表と内訳で足りるため何も足さない。
fn earliest_free_line(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    timezone: Option<&str>,
) -> Option<String> {
    if availabilities
        .iter()
        .any(|availability| is_free_at(availability, now))
    {
        return None;
    }

    let (resource, free_at) = availabilities
        .iter()
        .filter_map(|availability| {
            availability
                .next_free_at(now)
                .map(|free_at| (availability.resource(), free_at))
        })
        .min_by_key(|(_, free_at)| *free_at)?;

    Some(format!(
        "最も早く空くのは {}（{}）です。",
        format_resources_styled(std::slice::from_ref(resource), ResourceStyle::Compact),
        format_moment(free_at, now, timezone)
    ))
}

/// 表に渡すサーバーとデバイスの状態を設定順に集める
fn servers_of(availabilities: &[ResourceAvailability], now: DateTime<Utc>) -> Vec<grid::Server> {
    let mut servers: Vec<grid::Server> = Vec::new();

    for availability in availabilities {
        let Resource::Gpu(gpu) = availability.resource() else {
            continue;
        };
        let device = grid::Device {
            number: gpu.device_number(),
            is_free: is_free_at(availability, now),
        };

        match servers
            .iter_mut()
            .find(|server| server.name == gpu.server())
        {
            Some(server) => server.devices.push(device),
            None => servers.push(grid::Server {
                name: gpu.server().to_string(),
                devices: vec![device],
            }),
        }
    }

    servers
}

/// 部屋の空き状況（1部屋1行）
///
/// 部屋には番号の並びがないため表にはせず、使用中なら予約者と終了時刻もこの行に書く。
/// 部屋が設定されていなければ`None`。
fn room_lines(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    timezone: Option<&str>,
    owner_displays: &HashMap<EmailAddress, String>,
) -> Option<String> {
    let lines: Vec<String> = availabilities
        .iter()
        .filter_map(|availability| {
            let Resource::Room { name } = availability.resource() else {
                return None;
            };

            Some(match availability.state_at(now) {
                AvailabilityState::Free { .. } => format!("✅ {}", name),
                AvailabilityState::Busy(busy) => format!(
                    "🔴 {} — {} 〜{}",
                    name,
                    display_name(busy.owner_email(), owner_displays),
                    format_moment(busy.period().end(), now, timezone)
                ),
            })
        })
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(format!("*部屋*\n{}", lines.join("\n")))
}

/// 使用中のGPUを予約ごとにまとめた内訳
///
/// 使用中のGPUがなければ`None`。
fn ongoing_gpu_lines(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    timezone: Option<&str>,
    owner_displays: &HashMap<EmailAddress, String>,
) -> Option<String> {
    let reservations = ongoing_gpu_reservations(availabilities, now);

    if reservations.is_empty() {
        return None;
    }

    let lines: Vec<String> = reservations
        .iter()
        .map(|reservation| {
            format!(
                "🔴 {} — {} 〜{}",
                format_resources_styled(&reservation.resources, ResourceStyle::Compact)
                    .replace('\n', " / "),
                display_name(&reservation.owner_email, owner_displays),
                format_moment(reservation.ends_at, now, timezone)
            )
        })
        .collect();

    Some(format!("*使用中のGPU*\n{}", lines.join("\n")))
}

/// 内訳に並べる、いま動いている予約
struct OngoingReservation {
    owner_email: EmailAddress,
    resources: Vec<Resource>,
    ends_at: DateTime<Utc>,
}

/// 使用中のGPUを予約ごとにまとめる
///
/// ひとつの予約が複数のGPUを押さえていれば1件にまとめる。
/// 表と同じ順で読めるよう、最初に現れた順を保つ。
fn ongoing_gpu_reservations(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
) -> Vec<OngoingReservation> {
    let mut reservations: Vec<(String, OngoingReservation)> = Vec::new();

    for availability in availabilities {
        if !matches!(availability.resource(), Resource::Gpu(_)) {
            continue;
        }
        let AvailabilityState::Busy(busy) = availability.state_at(now) else {
            continue;
        };

        let usage_id = busy.usage_id().as_str().to_string();
        match reservations.iter_mut().find(|(id, _)| *id == usage_id) {
            Some((_, reservation)) => reservation.resources.push(availability.resource().clone()),
            None => reservations.push((
                usage_id,
                OngoingReservation {
                    owner_email: busy.owner_email().clone(),
                    resources: vec![availability.resource().clone()],
                    ends_at: busy.period().end(),
                },
            )),
        }
    }

    reservations
        .into_iter()
        .map(|(_, reservation)| reservation)
        .collect()
}

/// 予約者の表示名（解決できていなければメールアドレス）
fn display_name(email: &EmailAddress, owner_displays: &HashMap<EmailAddress, String>) -> String {
    owner_displays
        .get(email)
        .cloned()
        .unwrap_or_else(|| email.as_str().to_string())
}

/// 時刻を今日からの近さに応じて整形する
fn format_moment(instant: DateTime<Utc>, now: DateTime<Utc>, timezone: Option<&str>) -> String {
    let zoned = to_zoned(instant, timezone);
    let today = to_zoned(now, timezone).date_naive();
    let time = zoned.format("%H:%M");

    match zoned.date_naive().signed_duration_since(today).num_days() {
        0 => time.to_string(),
        1 => format!("明日 {}", time),
        2 => format!("明後日 {}", time),
        _ => format!("{} {}", zoned.format("%-m/%-d"), time),
    }
}

/// mrkdwnのセクションブロック
fn section(text: impl Into<String>) -> SlackBlock {
    let text: String = text.into();

    SlackBlock::Section(SlackSectionBlock::new().with_text(md!(text)))
}

/// mrkdwnのコンテキストブロック（本文より小さく薄い文字）
fn context(text: impl Into<String>) -> SlackBlock {
    let text: String = text.into();
    let element = SlackContextBlockElement::MarkDown(SlackBlockMarkDownText::new(text));

    SlackBlock::Context(SlackContextBlock::new(vec![element]))
}

#[cfg(test)]
mod tests;
