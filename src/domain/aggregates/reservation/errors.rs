use crate::domain::common::TimePeriodError;
use chrono::{DateTime, Utc};
use std::fmt;

/// Reservation集約のドメインエラー型
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ReservationError {
    /// 無効な期間
    InvalidTimePeriod(TimePeriodError),
    /// リソース項目が空
    NoResourceItems,
    /// まだ始まっていない予約枠を今で終了しようとした
    ///
    /// 使い始める前の予約枠には使った時間が存在しない。残すべき実績がないため、
    /// この場合に取れる操作は取り消しである。
    NotYetStarted {
        /// 予約枠の開始時刻
        start: DateTime<Utc>,
        /// 今で終了しようとした時刻
        at: DateTime<Utc>,
    },
    /// すでに終わった予約枠を今で終了しようとした
    AlreadyEnded {
        /// 予約枠の終了時刻
        end: DateTime<Utc>,
        /// 今で終了しようとした時刻
        at: DateTime<Utc>,
    },
}

impl fmt::Display for ReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReservationError::InvalidTimePeriod(e) => write!(f, "{}", e),
            ReservationError::NoResourceItems => {
                write!(f, "資源項目エラー: 少なくとも1つの資源項目が必要です")
            }
            ReservationError::NotYetStarted { start, at } => {
                write!(
                    f,
                    "まだ始まっていない予約枠です（開始時刻: {}、指定時刻: {}）。今で終了ではなく取り消してください。",
                    start.format("%Y-%m-%d %H:%M:%S"),
                    at.format("%Y-%m-%d %H:%M:%S")
                )
            }
            ReservationError::AlreadyEnded { end, at } => {
                write!(
                    f,
                    "すでに終わっている予約枠です（終了時刻: {}、指定時刻: {}）。",
                    end.format("%Y-%m-%d %H:%M:%S"),
                    at.format("%Y-%m-%d %H:%M:%S")
                )
            }
        }
    }
}

impl std::error::Error for ReservationError {}

impl From<TimePeriodError> for ReservationError {
    fn from(e: TimePeriodError) -> Self {
        ReservationError::InvalidTimePeriod(e)
    }
}

impl crate::domain::errors::DomainError for ReservationError {}
