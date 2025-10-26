use super::super::errors::ResourceUsageError;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimePeriod {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl TimePeriod {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, ResourceUsageError> {
        if start >= end {
            return Err(ResourceUsageError::InvalidTimePeriod { start, end });
        }
        Ok(Self { start, end })
    }

    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    // ? これは値オブジェクトのメソッドとして適切か？
    pub fn overlaps_with(&self, other: &TimePeriod) -> bool {
        self.start < other.end && other.start < self.end
    }
}
