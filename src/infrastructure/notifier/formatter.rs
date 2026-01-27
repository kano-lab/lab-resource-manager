//! 通知メッセージフォーマッター
//!
//! リソースと時刻のスタイル別フォーマット関数を提供します。

use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
use chrono::{Local, Utc};
use chrono_tz::Tz;
use std::collections::BTreeMap;

/// リソースをスタイルに応じてフォーマット
pub fn format_resources_styled(resources: &[Resource], style: ResourceStyle) -> String {
    match style {
        ResourceStyle::Full => format_resources_full(resources),
        ResourceStyle::Compact => format_resources_compact(resources),
        ResourceStyle::ServerOnly => format_resources_server_only(resources),
    }
}

/// フル形式: "Thalys / A100 80GB PCIe / GPU:0"
fn format_resources_full(resources: &[Resource]) -> String {
    crate::domain::aggregates::resource_usage::service::format_resources(resources)
}

/// コンパクト形式: "Thalys 0,1,2"（サーバーごとにグループ化）
fn format_resources_compact(resources: &[Resource]) -> String {
    let mut servers: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut rooms: Vec<&str> = Vec::new();

    for resource in resources {
        match resource {
            Resource::Gpu(gpu) => {
                servers
                    .entry(gpu.server())
                    .or_default()
                    .push(gpu.device_number());
            }
            Resource::Room { name } => {
                rooms.push(name);
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();

    for (server, mut devices) in servers {
        devices.sort_unstable();
        let devices_str = devices
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("{} {}", server, devices_str));
    }

    for room in rooms {
        parts.push(room.to_string());
    }

    parts.join("\n")
}

/// サーバー名のみ形式: "Thalys"（重複排除）
fn format_resources_server_only(resources: &[Resource]) -> String {
    use std::collections::BTreeSet;

    let mut servers: BTreeSet<&str> = BTreeSet::new();
    let mut rooms: BTreeSet<&str> = BTreeSet::new();

    for resource in resources {
        match resource {
            Resource::Gpu(gpu) => {
                servers.insert(gpu.server());
            }
            Resource::Room { name } => {
                rooms.insert(name);
            }
        }
    }

    servers
        .into_iter()
        .chain(rooms)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 時刻をスタイルに応じてフォーマット
pub fn format_time_styled(
    period: &TimePeriod,
    timezone_str: Option<&str>,
    style: TimeStyle,
    date_format: DateFormat,
) -> String {
    match style {
        TimeStyle::Full => format_time_full(period, timezone_str),
        TimeStyle::Smart => format_time_smart(period, timezone_str, date_format),
        TimeStyle::Relative => format_time_relative(period, timezone_str, date_format),
    }
}

/// フル形式: "2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo)"
fn format_time_full(period: &TimePeriod, timezone_str: Option<&str>) -> String {
    crate::domain::aggregates::resource_usage::service::format_time_period(period, timezone_str)
}

/// スマート形式: 同日なら終了日省略 "1/15 19:00-21:00"
fn format_time_smart(
    period: &TimePeriod,
    timezone_str: Option<&str>,
    date_format: DateFormat,
) -> String {
    let (start, end) = convert_to_timezone(period, timezone_str);

    let date_fmt = date_format_string(date_format);

    if start.date_naive() == end.date_naive() {
        // 同日: "1/15 19:00-21:00"
        format!(
            "{} {}-{}",
            start.format(date_fmt),
            start.format("%H:%M"),
            end.format("%H:%M")
        )
    } else {
        // 異日: "1/15 19:00 - 1/16 09:00"
        format!(
            "{} {} - {} {}",
            start.format(date_fmt),
            start.format("%H:%M"),
            end.format(date_fmt),
            end.format("%H:%M")
        )
    }
}

/// 相対形式: "今日 19:00-21:00", "明日 10:00-12:00"
fn format_time_relative(
    period: &TimePeriod,
    timezone_str: Option<&str>,
    date_format: DateFormat,
) -> String {
    let (start, end) = convert_to_timezone(period, timezone_str);
    let now = Utc::now();

    // 現在時刻を同じタイムゾーンで取得
    let now_date = if let Some(tz_str) = timezone_str {
        if let Ok(tz) = tz_str.parse::<Tz>() {
            now.with_timezone(&tz).date_naive()
        } else {
            now.with_timezone(&Local).date_naive()
        }
    } else {
        now.with_timezone(&Local).date_naive()
    };

    let start_date = start.date_naive();
    let days_diff = start_date.signed_duration_since(now_date).num_days();

    let date_str = match days_diff {
        0 => "今日".to_string(),
        1 => "明日".to_string(),
        2 => "明後日".to_string(),
        -1 => "昨日".to_string(),
        _ => start.format(date_format_string(date_format)).to_string(),
    };

    if start.date_naive() == end.date_naive() {
        format!(
            "{} {}-{}",
            date_str,
            start.format("%H:%M"),
            end.format("%H:%M")
        )
    } else {
        // 複数日にまたがる場合はスマート形式にフォールバック
        format_time_smart(period, timezone_str, date_format)
    }
}

/// 日付フォーマット文字列を取得
fn date_format_string(format: DateFormat) -> &'static str {
    match format {
        DateFormat::Ymd => "%Y-%m-%d",
        DateFormat::Md => "%-m/%-d",
        DateFormat::MdJapanese => "%-m月%-d日",
    }
}

/// TimePeriodを指定タイムゾーンに変換
fn convert_to_timezone(
    period: &TimePeriod,
    timezone_str: Option<&str>,
) -> (
    chrono::DateTime<chrono::FixedOffset>,
    chrono::DateTime<chrono::FixedOffset>,
) {
    if let Some(tz_str) = timezone_str
        && let Ok(tz) = tz_str.parse::<Tz>()
    {
        let start = period.start().with_timezone(&tz).fixed_offset();
        let end = period.end().with_timezone(&tz).fixed_offset();
        return (start, end);
    }

    let start = period.start().with_timezone(&Local).fixed_offset();
    let end = period.end().with_timezone(&Local).fixed_offset();
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use chrono::TimeZone;

    fn create_test_period(start_hour: u32, end_hour: u32) -> TimePeriod {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, start_hour, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, end_hour, 0, 0).unwrap();
        TimePeriod::new(start, end).unwrap()
    }

    fn create_multi_day_period() -> TimePeriod {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 22, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 16, 6, 0, 0).unwrap();
        TimePeriod::new(start, end).unwrap()
    }

    // Resource formatting tests

    #[test]
    fn test_format_resources_compact_single_server() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
        ];

        let result = format_resources_compact(&resources);
        assert_eq!(result, "Thalys 0,1,2"); // sorted
    }

    #[test]
    fn test_format_resources_compact_multiple_servers() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Freccia".to_string(), 1, "RTX6000".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "A100".to_string())),
        ];

        let result = format_resources_compact(&resources);
        assert_eq!(result, "Freccia 1\nThalys 0,2"); // BTreeMap orders alphabetically
    }

    #[test]
    fn test_format_resources_compact_with_room() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Room {
                name: "会議室A".to_string(),
            },
        ];

        let result = format_resources_compact(&resources);
        assert_eq!(result, "Thalys 0\n会議室A");
    }

    #[test]
    fn test_format_resources_server_only() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
            Resource::Gpu(Gpu::new("Freccia".to_string(), 0, "RTX6000".to_string())),
        ];

        let result = format_resources_server_only(&resources);
        assert_eq!(result, "Freccia\nThalys"); // BTreeSet orders and dedupes
    }

    #[test]
    fn test_format_resources_styled_full() {
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let result = format_resources_styled(&resources, ResourceStyle::Full);
        assert_eq!(result, "Thalys / A100 / GPU:0");
    }

    // Time formatting tests

    #[test]
    fn test_format_time_smart_same_day_jst() {
        // UTC 10:00-12:00 = JST 19:00-21:00
        let period = create_test_period(10, 12);

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::Md);
        assert_eq!(result, "1/15 19:00-21:00");
    }

    #[test]
    fn test_format_time_smart_same_day_japanese() {
        let period = create_test_period(10, 12);

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::MdJapanese);
        assert_eq!(result, "1月15日 19:00-21:00");
    }

    #[test]
    fn test_format_time_smart_multi_day() {
        // UTC 22:00 Jan 15 to 06:00 Jan 16 = JST 07:00 Jan 16 to 15:00 Jan 16
        let period = create_multi_day_period();

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::Md);
        // JST: Jan 16 07:00 - Jan 16 15:00 (same day in JST)
        assert_eq!(result, "1/16 07:00-15:00");
    }

    #[test]
    fn test_format_time_smart_ymd_format() {
        let period = create_test_period(10, 12);

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::Ymd);
        assert_eq!(result, "2024-01-15 19:00-21:00");
    }

    #[test]
    fn test_date_format_string() {
        assert_eq!(date_format_string(DateFormat::Ymd), "%Y-%m-%d");
        assert_eq!(date_format_string(DateFormat::Md), "%-m/%-d");
        assert_eq!(date_format_string(DateFormat::MdJapanese), "%-m月%-d日");
    }
}
