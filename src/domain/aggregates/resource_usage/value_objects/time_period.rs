use super::super::errors::ResourceUsageError;
use chrono::{DateTime, Utc};

/// 時間期間を表す値オブジェクト
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimePeriod {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl TimePeriod {
    /// 新しい時間期間を作成
    ///
    /// # Arguments
    /// * `start` - 開始時刻
    /// * `end` - 終了時刻
    ///
    /// # Errors
    /// 終了時刻が開始時刻より前の場合、`ResourceUsageError::InvalidTimePeriod`を返す
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, ResourceUsageError> {
        if start >= end {
            return Err(ResourceUsageError::InvalidTimePeriod { start, end });
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

    /// 他の時間期間と重複するかを判定
    pub fn overlaps_with(&self, other: &TimePeriod) -> bool {
        self.start < other.end && other.start < self.end
    }
}
