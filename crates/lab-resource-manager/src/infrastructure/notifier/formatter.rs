//! 通知メッセージフォーマッター
//!
//! リソースと時刻のスタイル別フォーマット関数を提供します。

use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::aggregates::reservation::{RecurrenceEnd, RecurrenceFrequency, RecurrenceRule};
use crate::domain::common::TimePeriod;
use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
use chrono::{DateTime, Local, Utc, Weekday};
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

/// フル形式: "Thalys / A100 80GB PCIe / GPU:0"（リソースごとに改行）
fn format_resources_full(resources: &[Resource]) -> String {
    resources
        .iter()
        .map(|resource| match resource {
            Resource::Gpu(spec) => format!(
                "{} / {} / GPU:{}",
                spec.server(),
                spec.model(),
                spec.device_number()
            ),
            Resource::Room { name } => name.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// 定期予約の繰り返しの間隔を文章にする
///
/// 例: "毎週 月・水（2026-12-31まで）", "2週間ごと 火", "毎日（全10回）"
pub fn format_recurrence(
    rule: &RecurrenceRule,
    timezone_str: Option<&str>,
    date_format: DateFormat,
) -> String {
    let mut text = match (rule.frequency(), rule.interval()) {
        (RecurrenceFrequency::Daily, 1) => "毎日".to_string(),
        (RecurrenceFrequency::Weekly, 1) => "毎週".to_string(),
        (RecurrenceFrequency::Monthly, 1) => "毎月".to_string(),
        (RecurrenceFrequency::Yearly, 1) => "毎年".to_string(),
        (RecurrenceFrequency::Daily, n) => format!("{n}日ごと"),
        (RecurrenceFrequency::Weekly, n) => format!("{n}週間ごと"),
        (RecurrenceFrequency::Monthly, n) => format!("{n}か月ごと"),
        (RecurrenceFrequency::Yearly, n) => format!("{n}年ごと"),
    };

    if !rule.weekdays().is_empty() {
        let weekdays: Vec<&str> = rule.weekdays().iter().map(weekday_label).collect();
        text.push_str(&format!(" {}", weekdays.join("・")));
    }

    match rule.end() {
        RecurrenceEnd::Never => {}
        RecurrenceEnd::Until(until) => text.push_str(&format!(
            "（{}まで）",
            to_zoned(until, timezone_str).format(date_format_string(date_format))
        )),
        RecurrenceEnd::AfterOccurrences(count) => text.push_str(&format!("（全{count}回）")),
    }

    text
}

fn weekday_label(weekday: &Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "月",
        Weekday::Tue => "火",
        Weekday::Wed => "水",
        Weekday::Thu => "木",
        Weekday::Fri => "金",
        Weekday::Sat => "土",
        Weekday::Sun => "日",
    }
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
        TimeStyle::Relative => format_time_relative(period, timezone_str, date_format, Utc::now()),
    }
}

/// フル形式: "2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo)"
///
/// タイムゾーンを指定した場合、そのタイムゾーンでの時刻とタイムゾーン名を表示する。
/// 指定がないか解釈できない場合は、システムのローカルタイムゾーンとそのオフセットで表示する。
fn format_time_full(period: &TimePeriod, timezone_str: Option<&str>) -> String {
    match timezone_str.and_then(|s| s.parse::<Tz>().ok()) {
        Some(tz) => format!(
            "{} - {} ({})",
            period.start().with_timezone(&tz).format("%Y-%m-%d %H:%M"),
            period.end().with_timezone(&tz).format("%Y-%m-%d %H:%M"),
            tz.name()
        ),
        None => {
            let start = period.start().with_timezone(&Local);
            let end = period.end().with_timezone(&Local);
            format!(
                "{} - {} ({})",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M"),
                start.offset()
            )
        }
    }
}

/// スマート形式: 同日なら終了日省略 "1/15 19:00 - 21:00"
fn format_time_smart(
    period: &TimePeriod,
    timezone_str: Option<&str>,
    date_format: DateFormat,
) -> String {
    let (start, end) = convert_to_timezone(period, timezone_str);

    let date_fmt = date_format_string(date_format);

    if start.date_naive() == end.date_naive() {
        // 同日: "1/15 19:00 - 21:00"
        format!(
            "{} {} - {}",
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

/// 相対形式: "今日 19:00 - 21:00", "明日 10:00 - 12:00"
///
/// 今日・明日は、`now`の日付（通知の時間帯で数える）からの日数で決める。
fn format_time_relative(
    period: &TimePeriod,
    timezone_str: Option<&str>,
    date_format: DateFormat,
    now: DateTime<Utc>,
) -> String {
    let (start, end) = convert_to_timezone(period, timezone_str);

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
            "{} {} - {}",
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

/// 時刻を指定タイムゾーンに変換
///
/// タイムゾーン名が未指定、または解釈できない場合はボットが動作している
/// 環境のローカルタイムゾーンを使う。
pub fn to_zoned(
    instant: chrono::DateTime<Utc>,
    timezone_str: Option<&str>,
) -> chrono::DateTime<chrono::FixedOffset> {
    if let Some(tz_str) = timezone_str
        && let Ok(tz) = tz_str.parse::<Tz>()
    {
        return instant.with_timezone(&tz).fixed_offset();
    }

    instant.with_timezone(&Local).fixed_offset()
}

/// 指定タイムゾーンでの、その日の始まり
///
/// 夏時間の切り替えで0時が存在しない日は、その日で最も早い時刻を返す。
///
/// # 引数
/// * `date` - 対象の日（指定タイムゾーンでの暦日）
/// * `timezone_str` - タイムゾーン名。未指定ならボットのローカルタイムゾーン
pub fn start_of_day(
    date: chrono::NaiveDate,
    timezone_str: Option<&str>,
) -> Option<chrono::DateTime<Utc>> {
    use chrono::TimeZone;

    let midnight = date.and_hms_opt(0, 0, 0)?;

    if let Some(tz_str) = timezone_str
        && let Ok(tz) = tz_str.parse::<Tz>()
    {
        return tz
            .from_local_datetime(&midnight)
            .earliest()
            .map(|zoned| zoned.with_timezone(&Utc));
    }

    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|zoned| zoned.with_timezone(&Utc))
}

/// TimePeriodを指定タイムゾーンに変換
fn convert_to_timezone(
    period: &TimePeriod,
    timezone_str: Option<&str>,
) -> (
    chrono::DateTime<chrono::FixedOffset>,
    chrono::DateTime<chrono::FixedOffset>,
) {
    (
        to_zoned(period.start(), timezone_str),
        to_zoned(period.end(), timezone_str),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RRULEの行から繰り返しの規則を作る
    fn rule(rrule: &str) -> RecurrenceRule {
        RecurrenceRule::parse(vec![format!("RRULE:{rrule}")]).unwrap()
    }

    #[test]
    fn a_weekly_recurrence_names_its_weekdays() {
        let weekly = rule("FREQ=WEEKLY;BYDAY=MO,WE");

        assert_eq!(
            format_recurrence(&weekly, Some("Asia/Tokyo"), DateFormat::Ymd),
            "毎週 月・水"
        );
    }

    #[test]
    fn a_longer_interval_is_spelled_out() {
        let biweekly = rule("FREQ=WEEKLY;INTERVAL=2;BYDAY=TU");
        let every_three_days = rule("FREQ=DAILY;INTERVAL=3");

        assert_eq!(
            format_recurrence(&biweekly, None, DateFormat::Ymd),
            "2週間ごと 火"
        );
        assert_eq!(
            format_recurrence(&every_three_days, None, DateFormat::Ymd),
            "3日ごと"
        );
    }

    #[test]
    fn the_end_of_a_recurrence_is_shown_in_the_notification_timezone() {
        // UTCでは12/31だが、日本時間では翌日の1/1にあたる
        let daily = rule("FREQ=DAILY;UNTIL=20261231T153000Z");

        assert_eq!(
            format_recurrence(&daily, Some("Asia/Tokyo"), DateFormat::Ymd),
            "毎日（2027-01-01まで）"
        );
    }

    #[test]
    fn a_recurrence_limited_by_count_says_how_many_times() {
        let monthly = rule("FREQ=MONTHLY;COUNT=10");

        assert_eq!(
            format_recurrence(&monthly, None, DateFormat::Ymd),
            "毎月（全10回）"
        );
    }

    use crate::domain::aggregates::reservation::value_objects::Gpu;
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
        assert_eq!(result, "1/15 19:00 - 21:00");
    }

    #[test]
    fn test_format_time_smart_same_day_japanese() {
        let period = create_test_period(10, 12);

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::MdJapanese);
        assert_eq!(result, "1月15日 19:00 - 21:00");
    }

    #[test]
    fn test_format_time_smart_multi_day() {
        // UTC 22:00 Jan 15 to 06:00 Jan 16 = JST 07:00 Jan 16 to 15:00 Jan 16
        let period = create_multi_day_period();

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::Md);
        // JST: Jan 16 07:00 - Jan 16 15:00 (same day in JST)
        assert_eq!(result, "1/16 07:00 - 15:00");
    }

    #[test]
    fn test_format_time_smart_ymd_format() {
        let period = create_test_period(10, 12);

        let result = format_time_smart(&period, Some("Asia/Tokyo"), DateFormat::Ymd);
        assert_eq!(result, "2024-01-15 19:00 - 21:00");
    }

    /// 1月`day`日の10時（UTC。東京では19時）から2時間
    fn evening_of(day: u32) -> TimePeriod {
        let start = Utc.with_ymd_and_hms(2024, 1, day, 10, 0, 0).unwrap();
        TimePeriod::new(start, start + chrono::Duration::hours(2)).unwrap()
    }

    #[test]
    fn relative_time_names_the_days_around_today() {
        // 今は1月15日の正午（東京）
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 3, 0, 0).unwrap();
        let relative = |day: u32| {
            format_time_relative(&evening_of(day), Some("Asia/Tokyo"), DateFormat::Md, now)
        };

        assert_eq!(relative(14), "昨日 19:00 - 21:00");
        assert_eq!(relative(15), "今日 19:00 - 21:00");
        assert_eq!(relative(16), "明日 19:00 - 21:00");
        assert_eq!(relative(17), "明後日 19:00 - 21:00");
        assert_eq!(relative(18), "1/18 19:00 - 21:00");
    }

    #[test]
    fn today_is_the_date_in_the_notification_timezone() {
        // UTCではまだ1月15日だが、東京ではもう16日の1時
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 16, 0, 0).unwrap();

        assert_eq!(
            format_time_relative(&evening_of(16), Some("Asia/Tokyo"), DateFormat::Md, now),
            "今日 19:00 - 21:00"
        );
    }

    #[test]
    fn test_date_format_string() {
        assert_eq!(date_format_string(DateFormat::Ymd), "%Y-%m-%d");
        assert_eq!(date_format_string(DateFormat::Md), "%-m/%-d");
        assert_eq!(date_format_string(DateFormat::MdJapanese), "%-m月%-d日");
    }

    // Full-style formatting tests

    #[test]
    fn full_time_without_a_timezone_uses_the_local_offset() {
        let period = create_test_period(10, 12);

        // テスト環境のタイムゾーンによって異なるため、形だけを確かめる
        let result = format_time_full(&period, None);
        assert!(result.contains("2024-01-15"));
        assert!(result.contains(" - "));
        assert!(result.contains('(') && result.contains(')'));
    }

    #[test]
    fn full_time_in_a_named_timezone_shows_its_iana_name() {
        let period = create_test_period(10, 12);

        assert_eq!(
            format_time_full(&period, Some("Asia/Tokyo")),
            "2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo)"
        );
        assert_eq!(
            format_time_full(&period, Some("America/New_York")),
            "2024-01-15 05:00 - 2024-01-15 07:00 (America/New_York)"
        );
    }

    #[test]
    fn full_time_with_an_unknown_timezone_falls_back_to_local() {
        let period = create_test_period(10, 12);

        let result = format_time_full(&period, Some("Invalid/Timezone"));
        assert!(result.contains("2024-01-15"));
        assert!(!result.contains("Invalid/Timezone"));
    }

    #[test]
    fn full_resources_put_one_resource_per_line() {
        let resources = vec![
            Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100 80GB PCIe".to_string(),
            )),
            Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                1,
                "A100 80GB PCIe".to_string(),
            )),
            Resource::Room {
                name: "会議室A".to_string(),
            },
        ];

        assert_eq!(
            format_resources_full(&resources),
            "Thalys / A100 80GB PCIe / GPU:0\nThalys / A100 80GB PCIe / GPU:1\n会議室A"
        );
        assert_eq!(format_resources_full(&[]), "");
    }
}
