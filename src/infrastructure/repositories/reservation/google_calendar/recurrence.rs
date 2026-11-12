//! Google Calendarの繰り返し指定（RFC 5545のRRULE）の読み取り (内部実装)

use crate::domain::aggregates::reservation::{RecurrenceEnd, RecurrenceFrequency, RecurrenceRule};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};

/// 繰り返し規則の行の接頭辞
const RRULE_PREFIX: &str = "RRULE:";

/// イベントの`recurrence`から繰り返しの間隔を読み取る
///
/// `recurrence`には除外日（EXDATE）なども並ぶが、間隔を表すのはRRULEの行だけである。
/// 読み取れない場合は`None`を返す。間隔が分からなくても定期予約であることは変わらないため、
/// 呼び出し側は間隔なしの定期予約として扱える。
pub(super) fn parse_recurrence(lines: &[String]) -> Option<RecurrenceRule> {
    lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(RRULE_PREFIX))
        .and_then(parse_rrule)
}

fn parse_rrule(rule: &str) -> Option<RecurrenceRule> {
    let mut frequency = None;
    let mut interval = 1;
    let mut weekdays = Vec::new();
    let mut end = RecurrenceEnd::Never;

    for part in rule.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => frequency = parse_frequency(value),
            "INTERVAL" => interval = value.trim().parse().ok()?,
            "BYDAY" => weekdays = value.split(',').filter_map(parse_weekday).collect(),
            "UNTIL" => end = RecurrenceEnd::Until(parse_until(value)?),
            "COUNT" => end = RecurrenceEnd::AfterOccurrences(value.trim().parse().ok()?),
            // 週の始まり（WKST）などは間隔の説明に影響しない
            _ => {}
        }
    }

    Some(RecurrenceRule::new(frequency?, interval, weekdays, end))
}

fn parse_frequency(value: &str) -> Option<RecurrenceFrequency> {
    match value.trim().to_ascii_uppercase().as_str() {
        "DAILY" => Some(RecurrenceFrequency::Daily),
        "WEEKLY" => Some(RecurrenceFrequency::Weekly),
        "MONTHLY" => Some(RecurrenceFrequency::Monthly),
        "YEARLY" => Some(RecurrenceFrequency::Yearly),
        _ => None,
    }
}

/// 曜日を読み取る
///
/// 「第2月曜」を表す`2MO`のように順序が付く場合があるため、末尾の2文字だけを見る。
fn parse_weekday(value: &str) -> Option<Weekday> {
    let value = value.trim().to_ascii_uppercase();
    match value.get(value.len().checked_sub(2)?..)? {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

/// 終了時刻を読み取る（`20261231T145959Z`または日付のみの`20261231`）
fn parse_until(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if let Ok(instant) = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ") {
        return Some(instant.and_utc());
    }
    NaiveDate::parse_from_str(value, "%Y%m%d")
        .ok()
        .map(|date| date.and_time(NaiveTime::MIN).and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_weekly_rule_carries_its_weekdays_interval_and_end() {
        let rule = parse_recurrence(&lines(&[
            "RRULE:FREQ=WEEKLY;WKST=SU;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261231T145959Z",
        ]))
        .unwrap();

        assert_eq!(rule.frequency(), RecurrenceFrequency::Weekly);
        assert_eq!(rule.interval(), 2);
        assert_eq!(rule.weekdays(), [Weekday::Mon, Weekday::Wed]);
        assert_eq!(
            rule.end(),
            RecurrenceEnd::Until(Utc.with_ymd_and_hms(2026, 12, 31, 14, 59, 59).unwrap())
        );
    }

    #[test]
    fn a_rule_without_details_repeats_every_time_without_end() {
        let rule = parse_recurrence(&lines(&["RRULE:FREQ=DAILY"])).unwrap();

        assert_eq!(rule.frequency(), RecurrenceFrequency::Daily);
        assert_eq!(rule.interval(), 1);
        assert!(rule.weekdays().is_empty());
        assert_eq!(rule.end(), RecurrenceEnd::Never);
    }

    #[test]
    fn a_rule_can_end_after_a_number_of_occurrences() {
        let rule = parse_recurrence(&lines(&["RRULE:FREQ=MONTHLY;COUNT=10;BYDAY=2MO"])).unwrap();

        assert_eq!(rule.end(), RecurrenceEnd::AfterOccurrences(10));
        assert_eq!(rule.weekdays(), [Weekday::Mon], "第2月曜も月曜として読む");
    }

    #[test]
    fn excluded_dates_listed_before_the_rule_do_not_hide_it() {
        let rule = parse_recurrence(&lines(&[
            "EXDATE;TZID=Asia/Tokyo:20261118T100000",
            "RRULE:FREQ=WEEKLY;UNTIL=20261231",
        ]))
        .unwrap();

        assert_eq!(
            rule.end(),
            RecurrenceEnd::Until(Utc.with_ymd_and_hms(2026, 12, 31, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn an_unreadable_recurrence_yields_no_rule() {
        assert_eq!(parse_recurrence(&lines(&["RRULE:FREQ=SECONDLY"])), None);
        assert_eq!(parse_recurrence(&lines(&["EXDATE:20261118"])), None);
        assert_eq!(parse_recurrence(&[]), None);
    }
}
