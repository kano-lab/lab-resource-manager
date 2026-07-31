use chrono::{DateTime, Utc};
use std::fmt;

/// ResourceUsage集約のドメインエラー型
#[derive(Debug, Clone)]
pub enum ResourceUsageError {
    /// 無効な時間枠（終了時刻が開始時刻より前）
    InvalidTimePeriod {
        /// 開始時刻
        start: DateTime<Utc>,
        /// 終了時刻
        end: DateTime<Utc>,
    },
    /// リソース項目が空
    NoResourceItems,
    /// まだ始まっていない予約を早期終了しようとした
    ///
    /// 使い始める前の予約には使った時間が存在しない。残すべき実績がないため、
    /// この場合に取れる操作は取り消しである。
    NotYetStarted {
        /// 予約の開始時刻
        start: DateTime<Utc>,
        /// 早期終了しようとした時刻
        at: DateTime<Utc>,
    },
    /// すでに終わった予約を早期終了しようとした
    AlreadyEnded {
        /// 予約の終了時刻
        end: DateTime<Utc>,
        /// 早期終了しようとした時刻
        at: DateTime<Utc>,
    },
    /// リソース使用の競合
    UsageConflict {
        /// 競合しているリソース名
        resource: String,
        /// 競合しているユーザー
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
            ResourceUsageError::NotYetStarted { start, at } => {
                write!(
                    f,
                    "まだ始まっていない予約です: 開始時刻({})は指定時刻({})よりあとです。早期終了ではなく取り消してください。",
                    start.format("%Y-%m-%d %H:%M:%S"),
                    at.format("%Y-%m-%d %H:%M:%S")
                )
            }
            ResourceUsageError::AlreadyEnded { end, at } => {
                write!(
                    f,
                    "すでに終わった予約です: 終了時刻({})は指定時刻({})より前です。",
                    end.format("%Y-%m-%d %H:%M:%S"),
                    at.format("%Y-%m-%d %H:%M:%S")
                )
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
