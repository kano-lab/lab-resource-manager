use crate::domain::aggregates::identity_link::errors::IdentityLinkError;
use crate::domain::aggregates::resource_usage::errors::ResourceUsageError;
use crate::domain::ports::{
    notifier::NotificationError, repositories::RepositoryError,
    resource_collection_access::ResourceCollectionAccessError,
};
use std::fmt;

#[derive(Debug)]
pub enum ApplicationError {
    // Infrastructure層からのエラー
    Repository(RepositoryError),
    Notification(NotificationError),
    ResourceCollectionAccess(ResourceCollectionAccessError),

    // Domain層からのエラー（集約ごと）
    ResourceUsage(ResourceUsageError),
    IdentityLink(IdentityLinkError),

    // UseCase固有のビジネスルール違反
    /// 外部システムが既に紐付けられている
    ExternalSystemAlreadyLinked {
        email: String,
        external_system: String,
    },
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplicationError::Repository(e) => write!(f, "リポジトリエラー: {}", e),
            ApplicationError::Notification(e) => write!(f, "通知エラー: {}", e),
            ApplicationError::ResourceCollectionAccess(e) => {
                write!(f, "リソースコレクションアクセスエラー: {}", e)
            }
            ApplicationError::ResourceUsage(e) => write!(f, "リソース使用エラー: {}", e),
            ApplicationError::IdentityLink(e) => write!(f, "ID紐付けエラー: {}", e),
            ApplicationError::ExternalSystemAlreadyLinked {
                email,
                external_system,
            } => {
                write!(
                    f,
                    "メールアドレス {} は既に {} に紐付けられています",
                    email, external_system
                )
            }
        }
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplicationError::Repository(e) => Some(e),
            ApplicationError::Notification(e) => Some(e),
            ApplicationError::ResourceCollectionAccess(e) => Some(e),
            ApplicationError::ResourceUsage(e) => Some(e),
            ApplicationError::IdentityLink(e) => Some(e),
            ApplicationError::ExternalSystemAlreadyLinked { .. } => None,
        }
    }
}

impl From<RepositoryError> for ApplicationError {
    fn from(e: RepositoryError) -> Self {
        ApplicationError::Repository(e)
    }
}

impl From<ResourceUsageError> for ApplicationError {
    fn from(e: ResourceUsageError) -> Self {
        ApplicationError::ResourceUsage(e)
    }
}

impl From<IdentityLinkError> for ApplicationError {
    fn from(e: IdentityLinkError) -> Self {
        ApplicationError::IdentityLink(e)
    }
}

impl From<NotificationError> for ApplicationError {
    fn from(e: NotificationError) -> Self {
        ApplicationError::Notification(e)
    }
}

impl From<ResourceCollectionAccessError> for ApplicationError {
    fn from(e: ResourceCollectionAccessError) -> Self {
        ApplicationError::ResourceCollectionAccess(e)
    }
}
