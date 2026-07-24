// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage, errors::DomainError, ports::PortError,
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
}

/// 通知サービスのポート
#[async_trait]
pub trait Notifier: Send + Sync {
    /// イベントを通知する
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError>;
}

/// 通知エラー
#[derive(Debug)]
pub enum NotificationError {
    /// 通知送信の失敗
    SendFailure(String),
    /// リポジトリエラー（IdentityLink取得失敗等）
    RepositoryError(String),
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationError::SendFailure(msg) => write!(f, "通知送信エラー: {}", msg),
            NotificationError::RepositoryError(msg) => {
                write!(f, "通知準備中のリポジトリエラー: {}", msg)
            }
        }
    }
}

impl std::error::Error for NotificationError {}
impl DomainError for NotificationError {}
impl PortError for NotificationError {}
