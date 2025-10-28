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
    /// メールアドレスが既に別のSlackユーザーに紐付けられている
    EmailAlreadyLinkedToAnotherUser {
        email: String,
    },
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplicationError::Repository(e) => write!(f, "Repository error: {}", e),
            ApplicationError::Notification(e) => write!(f, "Notification error: {}", e),
            ApplicationError::ResourceCollectionAccess(e) => {
                write!(f, "Resource collection access error: {}", e)
            }
            ApplicationError::ResourceUsage(e) => write!(f, "Resource usage error: {}", e),
            ApplicationError::IdentityLink(e) => write!(f, "Identity link error: {}", e),
            ApplicationError::EmailAlreadyLinkedToAnotherUser { email } => {
                write!(f, "Email {} is already linked to another Slack user", email)
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
            ApplicationError::EmailAlreadyLinkedToAnotherUser { .. } => None,
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
