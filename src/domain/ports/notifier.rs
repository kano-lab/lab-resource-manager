// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::recurring_reservation::RecurringReservation,
    aggregates::resource_usage::{entity::ResourceUsage, value_objects::Resource},
    common::EmailAddress,
    errors::DomainError,
    ports::PortError,
};
use async_trait::async_trait;
use std::fmt;

/// 通知イベントの種類
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    /// リソース使用予定が作成された
    ResourceUsageCreated(ResourceUsage),
    /// リソース使用予定が更新された
    ResourceUsageUpdated(ResourceUsage),
    /// リソース使用予定が削除された
    ResourceUsageDeleted(ResourceUsage),
    /// 定期予約が作成された
    ///
    /// 定期予約は回の数だけの予約ではなく1つの取り決めであり、1件として伝える。
    RecurringReservationCreated(RecurringReservation),
    /// 定期予約が変更された（時間帯や間隔の変更、途中からの打ち切りなど、複数の回に及ぶ変更）
    RecurringReservationUpdated(RecurringReservation),
    /// 定期予約が取り消された
    RecurringReservationCancelled(RecurringReservation),
}

impl NotificationEvent {
    /// イベントの対象となった予約の所有者
    pub fn owner_email(&self) -> &EmailAddress {
        match self {
            Self::ResourceUsageCreated(usage)
            | Self::ResourceUsageUpdated(usage)
            | Self::ResourceUsageDeleted(usage) => usage.owner_email(),
            Self::RecurringReservationCreated(reservation)
            | Self::RecurringReservationUpdated(reservation)
            | Self::RecurringReservationCancelled(reservation) => reservation.owner_email(),
        }
    }

    /// イベントの対象となった予約が使用するリソース
    pub fn resources(&self) -> &[Resource] {
        match self {
            Self::ResourceUsageCreated(usage)
            | Self::ResourceUsageUpdated(usage)
            | Self::ResourceUsageDeleted(usage) => usage.resources(),
            Self::RecurringReservationCreated(reservation)
            | Self::RecurringReservationUpdated(reservation)
            | Self::RecurringReservationCancelled(reservation) => reservation.resources(),
        }
    }

    /// イベントの対象となった予約（単発の予約・定期予約の1回、または定期予約）のID
    pub fn subject_id(&self) -> &str {
        match self {
            Self::ResourceUsageCreated(usage)
            | Self::ResourceUsageUpdated(usage)
            | Self::ResourceUsageDeleted(usage) => usage.id().as_str(),
            Self::RecurringReservationCreated(reservation)
            | Self::RecurringReservationUpdated(reservation)
            | Self::RecurringReservationCancelled(reservation) => reservation.id().as_str(),
        }
    }
}

/// 通知サービスのポート
#[async_trait]
pub trait Notifier: Send + Sync {
    /// イベントを通知する
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError>;
}

/// 通知エラー
#[derive(Debug)]
#[non_exhaustive]
pub enum NotificationError {
    /// 通知送信の失敗
    SendFailure(String),
    /// 宛先が分からない（IdentityLink未登録、Slackアカウント未リンク等）
    ///
    /// 送る手段の不調ではなく、送り先を知らないという状態。再試行しても届くように
    /// ならないため、呼び出し側は送信失敗とは別の扱いを選べる。
    RecipientUnknown(String),
    /// リポジトリエラー（IdentityLink取得失敗等）
    RepositoryError(String),
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationError::SendFailure(msg) => write!(f, "通知送信エラー: {}", msg),
            NotificationError::RecipientUnknown(msg) => {
                write!(f, "通知の宛先が分かりません: {}", msg)
            }
            NotificationError::RepositoryError(msg) => {
                write!(f, "通知準備中のリポジトリエラー: {}", msg)
            }
        }
    }
}

impl std::error::Error for NotificationError {}
impl DomainError for NotificationError {}
impl PortError for NotificationError {}
