use chrono::{DateTime, Utc};
use std::fmt;

/// EmailAddress Value Objectのエラー型
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EmailAddressError {
    /// '@'が含まれていない
    MissingAtSign,
}

impl fmt::Display for EmailAddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAtSign => write!(f, "無効なメールアドレス形式: '@'が含まれていません"),
        }
    }
}

impl std::error::Error for EmailAddressError {}

impl crate::domain::errors::DomainError for EmailAddressError {}

/// TimePeriod Value Objectのエラー型
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TimePeriodError {
    /// 終了時刻が開始時刻より後になっていない
    EndNotAfterStart {
        /// 開始時刻
        start: DateTime<Utc>,
        /// 終了時刻
        end: DateTime<Utc>,
    },
}

impl fmt::Display for TimePeriodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndNotAfterStart { start, end } => write!(
                f,
                "無効な時間枠: 終了時刻({})は開始時刻({})よりあとである必要があります。",
                end.format("%Y-%m-%d %H:%M:%S"),
                start.format("%Y-%m-%d %H:%M:%S")
            ),
        }
    }
}

impl std::error::Error for TimePeriodError {}

impl crate::domain::errors::DomainError for TimePeriodError {}
