use chrono::{DateTime, Utc};
use std::fmt;

#[derive(Debug, Clone)]
pub enum ResourceUsageError {
    InvalidTimePeriod {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    NoResourceItems,
    UsageConflict {
        resource: String,
        conflicting_user: String,
    },
}

impl fmt::Display for ResourceUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceUsageError::InvalidTimePeriod { start, end } => {
                write!(
                    f,
                    "無効な時間枠: 終了時刻({})は開始時刻({})よりあとである必要があります。",
                    end.format("%Y-%m-%d %H:%M:%S"),
                    start.format("%Y-%m-%d %H:%M:%S")
                )
            }
            ResourceUsageError::NoResourceItems => {
                write!(f, "資源項目エラー: 少なくとも1つの資源項目が必要です")
            }
            ResourceUsageError::UsageConflict {
                resource,
                conflicting_user,
            } => {
                write!(
                    f,
                    "使用予定の競合: {}が{}の使用予定と競合しています",
                    resource, conflicting_user
                )
            }
        }
    }
}

impl std::error::Error for ResourceUsageError {}

impl crate::domain::errors::DomainError for ResourceUsageError {}
