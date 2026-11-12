use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;
use rrule::RRuleSet;
use std::fmt;

/// 繰り返し規則の行の接頭辞
const RRULE_PREFIX: &str = "RRULE:";

/// 一度に展開する回の上限
///
/// 終わりのない繰り返しを長い期間で展開しても、際限なく回を作らないための歯止め。
/// 予約枠を引く期間は呼び出し側が区切るので、通常はこれに届かない。
const MAX_OCCURRENCES: u16 = 10_000;

/// 繰り返しの単位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrenceFrequency {
    /// 日ごと
    Daily,
    /// 週ごと
    Weekly,
    /// 月ごと
    Monthly,
    /// 年ごと
    Yearly,
}

/// 繰り返しがいつまで続くか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrenceEnd {
    /// 終わりを決めていない
    Never,
    /// この時刻まで
    Until(DateTime<Utc>),
    /// この回数まで
    AfterOccurrences(u32),
}

/// 繰り返しの規則
///
/// 「2週間ごとの月曜と水曜、年末まで」のように、何ごとに・何曜日に・いつまで繰り返すかを表す。
/// iCalendar（RFC 5545）の繰り返し指定（RRULE、除外日のEXDATEなど）の行をそのまま持ち、
/// 回の展開はその意味どおりに行う。何ごとに・何曜日に・いつまでは、知らせるときの説明に使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrenceRule {
    lines: Vec<String>,
    frequency: RecurrenceFrequency,
    interval: u32,
    weekdays: Vec<Weekday>,
    end: RecurrenceEnd,
}

/// 繰り返し指定を読めなかった
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrenceRuleError(String);

impl fmt::Display for RecurrenceRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "繰り返しの指定を読めません: {}", self.0)
    }
}

impl std::error::Error for RecurrenceRuleError {}

impl RecurrenceRule {
    /// iCalendarの繰り返し指定の行から、繰り返しの規則を読み取る
    ///
    /// # Arguments
    /// * `lines` - `RRULE:FREQ=WEEKLY;BYDAY=MO`のような行。EXDATEなどの行も含めてよい
    ///
    /// # Errors
    /// RRULEの行が無い、または回を展開できない指定の場合
    pub fn parse(lines: Vec<String>) -> Result<Self, RecurrenceRuleError> {
        let lines: Vec<String> = lines
            .into_iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        let rule = lines
            .iter()
            .find_map(|line| line.strip_prefix(RRULE_PREFIX))
            .ok_or_else(|| RecurrenceRuleError("RRULEの行がありません".to_string()))?;

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
                "INTERVAL" => {
                    interval = value
                        .trim()
                        .parse()
                        .map_err(|_| RecurrenceRuleError(format!("INTERVAL={value}")))?
                }
                "BYDAY" => weekdays = value.split(',').filter_map(parse_weekday).collect(),
                "UNTIL" => {
                    end = RecurrenceEnd::Until(
                        parse_until(value)
                            .ok_or_else(|| RecurrenceRuleError(format!("UNTIL={value}")))?,
                    )
                }
                "COUNT" => {
                    end = RecurrenceEnd::AfterOccurrences(
                        value
                            .trim()
                            .parse()
                            .map_err(|_| RecurrenceRuleError(format!("COUNT={value}")))?,
                    )
                }
                // 週の始まり（WKST）などは説明に影響しない。展開では使う
                _ => {}
            }
        }
        let frequency = frequency.ok_or_else(|| {
            RecurrenceRuleError(format!("FREQの無い、または未対応のRRULE: {rule}"))
        })?;

        let parsed = Self {
            lines,
            frequency,
            interval: interval.max(1),
            weekdays,
            end,
        };
        // 展開できない指定は、繰り返しの規則として受け取らない
        parsed.rule_set(DateTime::UNIX_EPOCH, Tz::UTC)?;
        Ok(parsed)
    }

    /// iCalendarの繰り返し指定の行
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// 繰り返しの単位
    pub fn frequency(&self) -> RecurrenceFrequency {
        self.frequency
    }

    /// 何単位ごとに繰り返すか（1以上）
    pub fn interval(&self) -> u32 {
        self.interval
    }

    /// 繰り返す曜日（指定がなければ空）
    pub fn weekdays(&self) -> &[Weekday] {
        &self.weekdays
    }

    /// 繰り返しがいつまで続くか
    pub fn end(&self) -> RecurrenceEnd {
        self.end
    }

    /// `from`から`to`まで（両端を含む）に始まる回の開始時刻を、早い順に求める
    ///
    /// # Arguments
    /// * `first_start` - 最初の回の開始時刻
    /// * `time_zone` - 繰り返しを数える時間帯。「毎週月曜10時」はこの時間帯の10時を指す
    pub fn occurrence_starts(
        &self,
        first_start: DateTime<Utc>,
        time_zone: Tz,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<DateTime<Utc>> {
        if to < from {
            return Vec::new();
        }
        let Ok(set) = self.rule_set(first_start, time_zone) else {
            // 受け取るときに展開できることを確かめているので、ここには来ない
            return Vec::new();
        };
        let offset = Self::subsecond_offset(first_start);
        set.after((from - offset).with_timezone(&rrule::Tz::UTC))
            .before((to - offset).with_timezone(&rrule::Tz::UTC))
            .all(MAX_OCCURRENCES)
            .dates
            .into_iter()
            .map(|start| start.with_timezone(&Utc) + offset)
            .collect()
    }

    /// `at`に始まる回があるか
    pub fn has_occurrence_at(
        &self,
        first_start: DateTime<Utc>,
        time_zone: Tz,
        at: DateTime<Utc>,
    ) -> bool {
        self.occurrence_starts(first_start, time_zone, at, at)
            .contains(&at)
    }

    /// 最後の回の開始時刻（終わりを決めていなければ`None`）
    pub fn last_start(&self, first_start: DateTime<Utc>, time_zone: Tz) -> Option<DateTime<Utc>> {
        if self.end == RecurrenceEnd::Never {
            return None;
        }
        let set = self.rule_set(first_start, time_zone).ok()?;
        set.all(MAX_OCCURRENCES)
            .dates
            .last()
            .map(|start| start.with_timezone(&Utc) + Self::subsecond_offset(first_start))
    }

    /// 最初の回の開始時刻の、秒未満の端数
    ///
    /// iCalendarの時刻は秒までしか書けないため、展開した回にはこの端数を足して戻す。
    /// 足さないと、最初の回でさえ、その開始時刻に始まる回が無いことになる。
    fn subsecond_offset(first_start: DateTime<Utc>) -> Duration {
        Duration::nanoseconds(i64::from(first_start.timestamp_subsec_nanos()))
    }

    fn rule_set(
        &self,
        first_start: DateTime<Utc>,
        time_zone: Tz,
    ) -> Result<RRuleSet, RecurrenceRuleError> {
        let local_start = first_start.with_timezone(&time_zone);
        let text = format!(
            "DTSTART;TZID={}:{}\n{}",
            time_zone.name(),
            local_start.format("%Y%m%dT%H%M%S"),
            self.lines.join("\n")
        );
        text.parse::<RRuleSet>()
            .map_err(|e| RecurrenceRuleError(e.to_string()))
    }
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

    fn rule(lines: &[&str]) -> RecurrenceRule {
        RecurrenceRule::parse(lines.iter().map(|line| line.to_string()).collect()).unwrap()
    }

    fn tokyo() -> Tz {
        "Asia/Tokyo".parse().unwrap()
    }

    /// 東京の時刻をUTCで
    fn jst(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        tokyo()
            .with_ymd_and_hms(y, m, d, h, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn reads_the_interval_weekdays_and_end_for_describing_the_rule() {
        let rule = rule(&["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261231T145959Z"]);

        assert_eq!(rule.frequency(), RecurrenceFrequency::Weekly);
        assert_eq!(rule.interval(), 2);
        assert_eq!(rule.weekdays(), &[Weekday::Mon, Weekday::Wed]);
        assert_eq!(
            rule.end(),
            RecurrenceEnd::Until(Utc.with_ymd_and_hms(2026, 12, 31, 14, 59, 59).unwrap())
        );
    }

    #[test]
    fn an_interval_of_zero_is_treated_as_every_time() {
        assert_eq!(rule(&["RRULE:FREQ=WEEKLY;INTERVAL=0"]).interval(), 1);
    }

    #[test]
    fn lines_without_a_readable_rule_are_refused() {
        for lines in [
            vec![],
            vec!["EXDATE:20261118T010000Z"],
            vec!["RRULE:FREQ=SOMETIMES"],
        ] {
            let lines = lines.into_iter().map(String::from).collect();
            assert!(RecurrenceRule::parse(lines).is_err());
        }
    }

    #[test]
    fn occurrences_are_counted_in_the_rules_time_zone() {
        // 2週間ごとの月曜と水曜、東京の10時。除外日を1つ持つ
        let rule = rule(&[
            "RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE",
            "EXDATE;TZID=Asia/Tokyo:20261118T100000",
        ]);
        let first = jst(2026, 11, 16, 10);

        let starts = rule.occurrence_starts(first, tokyo(), first, jst(2026, 12, 3, 0));

        assert_eq!(
            starts,
            vec![
                jst(2026, 11, 16, 10),
                jst(2026, 11, 30, 10),
                jst(2026, 12, 2, 10)
            ]
        );
        assert!(rule.has_occurrence_at(first, tokyo(), jst(2026, 11, 30, 10)));
        assert!(
            !rule.has_occurrence_at(first, tokyo(), jst(2026, 11, 18, 10)),
            "除外日は回ではない"
        );
        assert!(!rule.has_occurrence_at(first, tokyo(), jst(2026, 11, 30, 11)));
    }

    #[test]
    fn a_first_start_with_a_fraction_of_a_second_is_still_an_occurrence() {
        let first = jst(2026, 11, 16, 10) + Duration::milliseconds(250);
        let rule = rule(&["RRULE:FREQ=DAILY"]);

        assert!(rule.has_occurrence_at(first, tokyo(), first));
        assert!(rule.has_occurrence_at(first, tokyo(), first + Duration::days(1)));
    }

    #[test]
    fn a_rule_without_an_end_has_no_last_occurrence() {
        let first = jst(2026, 11, 16, 10);

        assert_eq!(rule(&["RRULE:FREQ=DAILY"]).last_start(first, tokyo()), None);
        assert_eq!(
            rule(&["RRULE:FREQ=DAILY;COUNT=3"]).last_start(first, tokyo()),
            Some(jst(2026, 11, 18, 10))
        );
    }
}
