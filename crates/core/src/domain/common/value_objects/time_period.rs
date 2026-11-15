use super::errors::TimePeriodError;
use chrono::{DateTime, Utc};

/// 期間を表す値オブジェクト
///
/// 開始時刻から終了時刻までの、長さを持つひとつながりの時間。予約枠が押さえる時間、
/// 空きを調べる範囲、実利用を観測した時間など、複数の集約やサービスで共有する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimePeriod {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl TimePeriod {
    /// 新しい期間を作成
    ///
    /// # Arguments
    /// * `start` - 開始時刻
    /// * `end` - 終了時刻
    ///
    /// # Errors
    /// 終了時刻が開始時刻より後でない場合、`TimePeriodError::EndNotAfterStart`を返す
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimePeriodError> {
        if start >= end {
            return Err(TimePeriodError::EndNotAfterStart { start, end });
        }
        Ok(Self { start, end })
    }

    /// 開始時刻を取得
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// 終了時刻を取得
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    /// 他の期間と重複するかを判定
    pub fn overlaps_with(&self, other: &TimePeriod) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, hour, 0, 0).unwrap()
    }

    fn period(start: u32, end: u32) -> TimePeriod {
        TimePeriod::new(at(start), at(end)).unwrap()
    }

    #[test]
    fn periods_that_only_touch_do_not_overlap() {
        // 12時に終わる予約のすぐ後に、12時から始まる予約を入れられる
        let (morning, afternoon) = (period(9, 12), period(12, 15));

        assert!(!morning.overlaps_with(&afternoon));
        assert!(!afternoon.overlaps_with(&morning));
    }

    #[test]
    fn periods_that_share_any_moment_overlap() {
        let morning = period(9, 12);
        let one_nanosecond_early =
            TimePeriod::new(at(12) - Duration::nanoseconds(1), at(15)).unwrap();

        assert!(morning.overlaps_with(&one_nanosecond_early));
        assert!(one_nanosecond_early.overlaps_with(&morning));
    }

    #[test]
    fn a_period_overlaps_one_it_contains_and_itself() {
        let day = period(9, 18);
        let lunch = period(12, 13);

        assert!(day.overlaps_with(&lunch));
        assert!(lunch.overlaps_with(&day));
        assert!(day.overlaps_with(&day));
    }

    #[test]
    fn a_period_must_end_after_it_starts() {
        assert_eq!(
            TimePeriod::new(at(9), at(9)),
            Err(TimePeriodError::EndNotAfterStart {
                start: at(9),
                end: at(9)
            })
        );
        assert!(TimePeriod::new(at(10), at(9)).is_err());
    }
}
