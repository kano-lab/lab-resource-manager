// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::reservation::{Reservation, value_objects::Resource},
    common::EmailAddress,
    errors::DomainError,
    ports::PortError,
};
use async_trait::async_trait;
use std::fmt;

/// 通知イベントの種類
///
/// 予約は作る・取り消すの単位であり、通知も予約を単位とする。
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    /// 予約が作られた
    ReservationCreated(Reservation),
    /// 予約が変更された（予約枠の変更や今で終了を含む）
    ReservationUpdated(Reservation),
    /// 予約が取り消された
    ReservationCancelled(Reservation),
}

impl NotificationEvent {
    /// イベントの対象の予約者
    pub fn holder_email(&self) -> &EmailAddress {
        match self {
            Self::ReservationCreated(reservation)
            | Self::ReservationUpdated(reservation)
            | Self::ReservationCancelled(reservation) => reservation.holder_email(),
        }
    }

    /// イベントの対象が押さえるリソース
    pub fn resources(&self) -> &[Resource] {
        match self {
            Self::ReservationCreated(reservation)
            | Self::ReservationUpdated(reservation)
            | Self::ReservationCancelled(reservation) => reservation.resources(),
        }
    }

    /// イベントの対象の予約のID
    pub fn subject_id(&self) -> &str {
        match self {
            Self::ReservationCreated(reservation)
            | Self::ReservationUpdated(reservation)
            | Self::ReservationCancelled(reservation) => reservation.id().as_str(),
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
