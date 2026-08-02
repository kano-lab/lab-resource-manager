//! Slack DM経由の未使用予約の通知実装

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Gpu;
use crate::domain::ports::idle_reservation_notifier::{
    IdleEvidence, IdleReservation, IdleReservationNotifier,
};
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};
use crate::infrastructure::slack_direct_message::SlackDirectMessenger;
use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use slack_morphism::prelude::*;
use std::sync::Arc;
use tracing::warn;

use crate::interface::slack::constants::{
    ACTION_IDLE_CANCEL, ACTION_IDLE_KEEP, ACTION_IDLE_RELEASE,
};

/// Slack DM経由で未使用の予約を予約者へ知らせる実装
pub struct SlackIdleReservationNotifier {
    messenger: SlackDirectMessenger,
}

impl SlackIdleReservationNotifier {
    /// 新しい実装を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            messenger: SlackDirectMessenger::new(slack_client, bot_token, identity_repo),
        }
    }
}

#[async_trait]
impl IdleReservationNotifier for SlackIdleReservationNotifier {
    async fn notify_idle(&self, idle: IdleReservation) -> Result<(), NotificationError> {
        let reservation = idle.reservation();
        let text = build_idle_message(reservation, idle.idle_since(), idle.evidence());
        let blocks = build_idle_blocks(reservation, &text);

        let sent = self
            .messenger
            .send(reservation.owner_email(), text, blocks)
            .await?;

        tracing::info!(
            channel = %sent.channel,
            ts = %sent.ts,
            usage_id = %reservation.id().as_str(),
            recipient = %reservation.owner_email().as_str(),
            idle_since = %idle.idle_since(),
            evidence = ?idle.evidence(),
            "sent an idle-reservation dm"
        );

        Ok(())
    }
}

/// 未使用の予約を知らせるDMの本文を構築する（純粋関数、ユニットテスト対象）
fn build_idle_message(
    reservation: &ResourceUsage,
    idle_since: DateTime<Utc>,
    evidence: &IdleEvidence,
) -> String {
    let resources = format_resources_styled(reservation.resources(), ResourceStyle::Full);
    let period = format_time_styled(
        reservation.time_period(),
        None,
        TimeStyle::Full,
        DateFormat::Ymd,
    );

    format!(
        "{}\n\n💻 予約リソース\n{}\n\n📅 予約期間\n{}\n\n{}以降、{}{}",
        headline_of(evidence),
        resources,
        period,
        idle_since.with_timezone(&Local).format("%H:%M"),
        observation_of(evidence),
        advice_of(evidence),
    )
}

/// 何が起きているかを一行で言い切る見出し
fn headline_of(evidence: &IdleEvidence) -> &'static str {
    match evidence {
        IdleEvidence::NoProcesses => "⏳ 予約したリソースが使われていません",
        IdleEvidence::HeldWithoutComputing { .. } if evidence.is_partial() => {
            "⏳ 予約したGPUの一部で計算が走っていません"
        }
        IdleEvidence::HeldWithoutComputing { .. } => "⏳ 予約したGPUで計算が走っていません",
    }
}

/// そう判断した根拠を、予約者が心当たりと突き合わせられる形で述べる
fn observation_of(evidence: &IdleEvidence) -> String {
    match evidence {
        IdleEvidence::NoProcesses => "あなたの利用を確認できていません。".to_string(),
        IdleEvidence::HeldWithoutComputing {
            at_rest,
            peak_utilization_percent,
            used_memory_mib,
            ..
        } => format!(
            "{}で計算が走っていません（最大稼働率 {}%{}）。",
            format_devices(at_rest),
            peak_utilization_percent,
            match used_memory_mib {
                Some(mib) => format!("、確保 {}", format_memory(*mib)),
                None => String::new(),
            }
        ),
    }
}

/// どのGPUのことなのかを、予約者が現場で確かめられる呼び方で示す
fn format_devices(at_rest: &[Gpu]) -> String {
    let devices: Vec<String> = at_rest
        .iter()
        .map(|gpu| format!("{} GPU{}", gpu.server(), gpu.device_number()))
        .collect();

    devices.join("・")
}

/// 使い終わっているなら何ができるのかを、状況に合う言葉で示す
fn advice_of(evidence: &IdleEvidence) -> &'static str {
    if evidence.is_partial() {
        // 一部だけを返す操作はないため、残りを使い続けるなら今のままでよい
        return "残りを使い続けるならそのままで構いません。まとめて使い終わっているなら、いったん終了して必要な分だけ取り直せます。";
    }
    "使い終わっているなら、残りの時間を他の人に開放できます。"
}

/// メモリ量を、確保の大きさが一目で分かる単位で表す
fn format_memory(used_memory_mib: u64) -> String {
    if used_memory_mib < 1024 {
        return format!("{} MiB", used_memory_mib);
    }
    format!("{:.1} GiB", used_memory_mib as f64 / 1024.0)
}

/// 未使用の予約を知らせるDMのBlock Kitメッセージを構築する（純粋関数、ユニットテスト対象）
fn build_idle_blocks(reservation: &ResourceUsage, text: &str) -> Vec<SlackBlock> {
    let usage_id = reservation.id().as_str();

    let blocks_json = serde_json::json!([
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": text
            }
        },
        {
            "type": "actions",
            "elements": [
                {
                    "type": "button",
                    "text": {
                        "type": "plain_text",
                        "text": "⏹️ 今で終了する"
                    },
                    "style": "primary",
                    "action_id": ACTION_IDLE_RELEASE,
                    "value": usage_id
                },
                {
                    "type": "button",
                    "text": {
                        "type": "plain_text",
                        "text": "✅ まだ使う"
                    },
                    "action_id": ACTION_IDLE_KEEP,
                    "value": usage_id
                },
                {
                    "type": "button",
                    "text": {
                        "type": "plain_text",
                        "text": "❌ 予約を取り消す"
                    },
                    "style": "danger",
                    "action_id": ACTION_IDLE_CANCEL,
                    "value": usage_id
                }
            ]
        }
    ]);

    serde_json::from_value(blocks_json).unwrap_or_else(|e| {
        warn!(error = %e, "building the idle reservation message blocks failed");
        vec![]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use crate::domain::common::EmailAddress;
    use chrono::Duration;

    fn sample_reservation() -> ResourceUsage {
        let start = Utc::now() - Duration::hours(2);
        ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
            TimePeriod::new(start, start + Duration::hours(6)).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[test]
    fn the_message_names_the_resource_and_when_it_went_quiet() {
        let reservation = sample_reservation();
        let idle_since = Utc::now() - Duration::hours(1);

        let message = build_idle_message(&reservation, idle_since, &IdleEvidence::NoProcesses);

        assert!(message.contains("Thalys"), "{message}");
        assert!(
            message.contains(&idle_since.with_timezone(&Local).format("%H:%M").to_string()),
            "いつから使われていないのかが分からないと、心当たりを確かめられない: {message}"
        );
    }

    #[test]
    fn a_gpu_held_without_computing_is_told_apart_from_one_nobody_touched() {
        let reservation = sample_reservation();
        let idle_since = Utc::now() - Duration::hours(1);

        let held = build_idle_message(
            &reservation,
            idle_since,
            &IdleEvidence::HeldWithoutComputing {
                at_rest: vec![Gpu::new("Thalys".to_string(), 0, "A100".to_string())],
                observed_count: 1,
                peak_utilization_percent: 3,
                used_memory_mib: Some(38_000),
            },
        );
        let absent = build_idle_message(&reservation, idle_since, &IdleEvidence::NoProcesses);

        assert_ne!(
            held, absent,
            "心当たりの違う二つの状況を、同じ文面で知らせてはいけない"
        );
        assert!(
            held.contains("37.1 GiB"),
            "確保されている量が分かる: {held}"
        );
        assert!(
            held.contains("3%"),
            "稼働率まで示して確かめられるようにする: {held}"
        );
    }

    #[test]
    fn memory_is_shown_in_a_unit_that_reads_at_a_glance() {
        assert_eq!(format_memory(38_000), "37.1 GiB");
        assert_eq!(format_memory(512), "512 MiB");
    }

    #[test]
    fn every_button_carries_the_reservation_it_acts_on() {
        let reservation = sample_reservation();

        let blocks = build_idle_blocks(&reservation, "test message");
        let json = serde_json::to_value(&blocks).unwrap();
        let elements = json[1]["elements"].as_array().unwrap();

        assert_eq!(elements.len(), 3, "終了・継続・取り消しの3択: {json}");
        for element in elements {
            assert_eq!(
                element["value"].as_str(),
                Some(reservation.id().as_str()),
                "どの予約への操作か分からないボタンがある: {element}"
            );
        }
    }

    #[test]
    fn the_buttons_are_distinguishable_from_one_another() {
        let reservation = sample_reservation();

        let blocks = build_idle_blocks(&reservation, "test message");
        let json = serde_json::to_value(&blocks).unwrap();
        let action_ids: Vec<&str> = json[1]["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|element| element["action_id"].as_str().unwrap())
            .collect();

        // Slackはブロック内でaction_idが重複するとinvalid_blocksでメッセージ全体を拒否する
        let unique: std::collections::HashSet<&&str> = action_ids.iter().collect();
        assert_eq!(unique.len(), action_ids.len(), "{action_ids:?}");
    }
}
