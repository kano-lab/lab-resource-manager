// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{UsageId, User},
    },
    errors::DomainError,
    ports::PortError,
};
use async_trait::async_trait;
use std::fmt;

#[derive(Debug, Clone)]
pub enum NotificationEvent {
    ResourceUsageCreated(ResourceUsage),
    ResourceUsageUpdated(ResourceUsage),
    ResourceUsageDeleted { id: UsageId, user: User },
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError>;
}

#[derive(Debug)]
pub struct NotificationError {
    pub message: String,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "通知エラー: {}", self.message)
    }
}

impl std::error::Error for NotificationError {}
impl DomainError for NotificationError {}
impl PortError for NotificationError {}
