use crate::domain::aggregates::resource_usage::errors::ResourceUsageError;
use crate::domain::ports::{notifier::NotificationError, repositories::RepositoryError};
use std::fmt;

#[derive(Debug)]
pub enum ApplicationError {
    Repository(RepositoryError),
    Domain(ResourceUsageError),
    Notification(NotificationError),
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplicationError::Repository(e) => write!(f, "{}", e),
            ApplicationError::Domain(e) => write!(f, "{}", e),
            ApplicationError::Notification(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplicationError::Repository(e) => Some(e),
            ApplicationError::Domain(e) => Some(e),
            ApplicationError::Notification(e) => Some(e),
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
        ApplicationError::Domain(e)
    }
}

impl From<NotificationError> for ApplicationError {
    fn from(e: NotificationError) -> Self {
        ApplicationError::Notification(e)
    }
}
