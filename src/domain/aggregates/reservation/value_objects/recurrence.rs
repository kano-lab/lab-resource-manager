use chrono::{DateTime, Utc, Weekday};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrenceRule {
    frequency: RecurrenceFrequency,
    interval: u32,
    weekdays: Vec<Weekday>,
    end: RecurrenceEnd,
}

impl RecurrenceRule {
    /// 新しい繰り返しの規則を作成する
    ///
    /// # Arguments
    /// * `frequency` - 繰り返しの単位
    /// * `interval` - 何単位ごとに繰り返すか。0は意味を持たないため1として扱う
    /// * `weekdays` - 繰り返す曜日（指定がなければ空）
    /// * `end` - 繰り返しがいつまで続くか
    pub fn new(
        frequency: RecurrenceFrequency,
        interval: u32,
        weekdays: Vec<Weekday>,
        end: RecurrenceEnd,
    ) -> Self {
        Self {
            frequency,
            interval: interval.max(1),
            weekdays,
            end,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interval_of_zero_is_treated_as_every_time() {
        let rule =
            RecurrenceRule::new(RecurrenceFrequency::Weekly, 0, vec![], RecurrenceEnd::Never);

        assert_eq!(rule.interval(), 1);
    }
}
