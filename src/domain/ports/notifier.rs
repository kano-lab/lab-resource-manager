// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage, errors::DomainError, ports::PortError,
};
use async_trait::async_trait;
use std::fmt;

#[derive(Debug, Clone)]
pub enum NotificationEvent {
    ResourceUsageCreated(ResourceUsage),
    ResourceUsageUpdated(ResourceUsage),
    ResourceUsageDeleted(ResourceUsage),
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError>;
}

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
