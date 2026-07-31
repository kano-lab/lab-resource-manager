//! 予約をリソース×時間の表示モデルへ変換する
//!
//! カレンダーは「1人の予定を時間軸で見る」ためのUIで、複数リソースの空きを横並びで
//! 比べる用途には向かない。ここでは縦にリソース、横に時間を取った表を組み立てる。
//!
//! 位置は表示範囲に対する割合（0.0〜1.0）で持つ。ピクセルやグリッドの列数に落とすのは
//! 描画側の仕事で、ここでは時間の比率だけを扱う。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::infrastructure::config::ResourceConfig;
use chrono::{DateTime, Duration, Timelike, Utc};
use chrono_tz::Tz;

/// 行の同一性
///
/// `Resource`をそのまま鍵にしない。`Resource::Gpu`の等価判定はGPUのモデル名まで含むため、
/// 設定ファイルの表記と永続化層から復元された表記が食い違うと同じGPUが別行に割れてしまう。
/// 行を決めるのはサーバー名とデバイス番号であって、モデル名は表示のための情報でしかない。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RowKey {
    Gpu { server: String, device: u32 },
    Room { name: String },
}

impl RowKey {
    fn of(resource: &Resource) -> Self {
        match resource {
            Resource::Gpu(gpu) => RowKey::Gpu {
                server: gpu.server().to_string(),
                device: gpu.device_number(),
            },
            Resource::Room { name } => RowKey::Room { name: name.clone() },
        }
    }
}

/// 表示範囲に収めた予約1件
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineBlock {
    pub id: String,
    /// 所有者。メールアドレス全体は出さず、ローカルパートのみ
    pub owner: String,
    pub notes: Option<String>,
    /// 表示タイムゾーンでの開始・終了（ツールチップ用の実際の時刻）
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    /// 表示範囲に対する位置（0.0〜1.0）
    pub start_ratio: f64,
    pub end_ratio: f64,
    /// 表示範囲の外から続いている／外へ続いていく
    pub clipped_start: bool,
    pub clipped_end: bool,
}

impl TimelineBlock {
    pub fn width_ratio(&self) -> f64 {
        self.end_ratio - self.start_ratio
    }
}

/// タイムラインの1行（1リソース）
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRow {
    pub key: RowKey,
    /// 行の見出し（例: "GPU#0 (A100)"）
    pub label: String,
    /// 行のまとまり（例: サーバー名）。連続する同じ値はまとめて表示できる
    pub group: String,
    /// 時間が重なる予約は別レーンへ分ける。潰して隠さない
    pub lanes: Vec<Vec<TimelineBlock>>,
}

impl TimelineRow {
    fn new(key: RowKey, label: String, group: String) -> Self {
        Self {
            key,
            label,
            group,
            lanes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lanes.iter().all(|lane| lane.is_empty())
    }
}

/// 時間軸の目盛り
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub label: String,
    pub ratio: f64,
    /// 日の変わり目。補助目盛りより強く描く
    pub is_day_boundary: bool,
}

/// 画面に渡す表示モデル一式
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pub rows: Vec<TimelineRow>,
    pub ticks: Vec<Tick>,
    /// 表示範囲の開始・終了（表示タイムゾーン）
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    /// 現在時刻の位置。表示範囲の外なら`None`
    pub now_ratio: Option<f64>,
}

/// 予約と設定からタイムラインを組み立てる
pub fn build(
    config: &ResourceConfig,
    usages: &[ResourceUsage],
    window: &TimePeriod,
    now: DateTime<Utc>,
    tz: Tz,
) -> Timeline {
    let mut rows = configured_rows(config);
    place_usages(&mut rows, usages, window, tz);

    Timeline {
        rows,
        ticks: build_ticks(window, tz),
        start: window.start().with_timezone(&tz),
        end: window.end().with_timezone(&tz),
        now_ratio: ratio_within(now, window),
    }
}

/// 設定に書かれたリソースを行の骨格にする
///
/// 予約が1件も無いリソースも行として残す。空いていることが見えるのが目的なので、
/// 予約のあるリソースだけを並べたのでは表として意味をなさない。
fn configured_rows(config: &ResourceConfig) -> Vec<TimelineRow> {
    let gpus = config.servers.iter().flat_map(|server| {
        server.devices.iter().map(move |device| {
            TimelineRow::new(
                RowKey::Gpu {
                    server: server.name.clone(),
                    device: device.id,
                },
                format!("GPU#{} ({})", device.id, device.model),
                server.name.clone(),
            )
        })
    });

    let rooms = config.rooms.iter().map(|room| {
        TimelineRow::new(
            RowKey::Room {
                name: room.name.clone(),
            },
            room.name.clone(),
            "部屋".to_string(),
        )
    });

    gpus.chain(rooms).collect()
}

/// 予約を対応する行へ配り、行ごとにレーンを割り当てる
///
/// 設定に無いリソースの予約は捨てずに行を足して受け止める。設定から外れたサーバーの
/// 予約が黙って消えると、画面が「空いている」と嘘をつくことになる。
fn place_usages(
    rows: &mut Vec<TimelineRow>,
    usages: &[ResourceUsage],
    window: &TimePeriod,
    tz: Tz,
) {
    for usage in usages {
        let block = to_block(usage, window, tz);

        for resource in usage.resources() {
            let key = RowKey::of(resource);
            let index = match rows.iter().position(|row| row.key == key) {
                Some(index) => index,
                None => {
                    rows.push(TimelineRow::new(
                        key,
                        resource.to_string(),
                        "設定にないリソース".to_string(),
                    ));
                    rows.len() - 1
                }
            };

            push_into_lane(&mut rows[index].lanes, block.clone());
        }
    }
}

/// 既存のどのブロックとも重ならないレーンへ入れる。無ければ新しいレーンを作る
fn push_into_lane(lanes: &mut Vec<Vec<TimelineBlock>>, block: TimelineBlock) {
    for lane in lanes.iter_mut() {
        let fits = lane.iter().all(|placed| {
            placed.end_ratio <= block.start_ratio || block.end_ratio <= placed.start_ratio
        });

        if fits {
            lane.push(block);
            return;
        }
    }

    lanes.push(vec![block]);
}

/// 予約を表示範囲で切り取ってブロックにする
fn to_block(usage: &ResourceUsage, window: &TimePeriod, tz: Tz) -> TimelineBlock {
    let period = usage.time_period();
    let total = window_seconds(window);

    let start_offset = (period.start() - window.start()).num_seconds() as f64;
    let end_offset = (period.end() - window.start()).num_seconds() as f64;

    TimelineBlock {
        id: usage.id().as_str().to_string(),
        owner: local_part(usage.owner_email().as_str()),
        notes: usage.notes().cloned(),
        start: period.start().with_timezone(&tz),
        end: period.end().with_timezone(&tz),
        start_ratio: (start_offset / total).clamp(0.0, 1.0),
        end_ratio: (end_offset / total).clamp(0.0, 1.0),
        clipped_start: period.start() < window.start(),
        clipped_end: period.end() > window.end(),
    }
}

/// メールアドレスのローカルパート
///
/// 画面を見るのに認証を求めない代わり、アドレス全体は出さない。
fn local_part(email: &str) -> String {
    email.split('@').next().unwrap_or(email).to_string()
}

/// 時間軸の目盛りを作る
///
/// 日の変わり目は必ず引き、表示期間が短いときだけ時刻の補助目盛りを足す。
fn build_ticks(window: &TimePeriod, tz: Tz) -> Vec<Tick> {
    let span_hours = (window.end() - window.start()).num_hours();
    let step = tick_step_hours(span_hours);

    let mut ticks = Vec::new();
    let mut at = first_tick_at(window.start().with_timezone(&tz), step);

    while at < window.end().with_timezone(&tz) {
        let is_day_boundary = at.hour() == 0;
        let label = if is_day_boundary {
            at.format("%-m/%-d (%a)").to_string()
        } else {
            at.format("%-H:%M").to_string()
        };

        if let Some(ratio) = ratio_within(at.with_timezone(&Utc), window) {
            ticks.push(Tick {
                label,
                ratio,
                is_day_boundary,
            });
        }

        at += Duration::hours(step);
    }

    ticks
}

/// 目盛りの間隔。表示が詰まりすぎない程度に粗くする
fn tick_step_hours(span_hours: i64) -> i64 {
    match span_hours {
        ..=24 => 3,
        25..=72 => 6,
        73..=240 => 12,
        _ => 24,
    }
}

/// 表示開始以降で最初に目盛りが来る時刻へ切り上げる
fn first_tick_at(start: DateTime<Tz>, step_hours: i64) -> DateTime<Tz> {
    let truncated = start
        .with_minute(0)
        .and_then(|at| at.with_second(0))
        .and_then(|at| at.with_nanosecond(0))
        .unwrap_or(start);

    let remainder = truncated.hour() as i64 % step_hours;
    let aligned = truncated - Duration::hours(remainder);

    if aligned < start {
        aligned + Duration::hours(step_hours)
    } else {
        aligned
    }
}

/// 表示範囲に対する位置。範囲外なら`None`
fn ratio_within(at: DateTime<Utc>, window: &TimePeriod) -> Option<f64> {
    if at < window.start() || at > window.end() {
        return None;
    }

    let offset = (at - window.start()).num_seconds() as f64;
    Some(offset / window_seconds(window))
}

/// 表示範囲の長さ（秒）。ゼロ除算を避けるため最低1秒とみなす
fn window_seconds(window: &TimePeriod) -> f64 {
    ((window.end() - window.start()).num_seconds() as f64).max(1.0)
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
