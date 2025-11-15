//! リソース使用ドメインサービスのエラー

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::errors::DomainError;
use crate::domain::ports::repositories::RepositoryError;
use std::fmt;

/// リソース競合エラー
#[derive(Debug)]
pub struct ResourceConflictError {
    /// 競合しているリソースの説明
    pub resource_description: String,
    /// 競合している既存の使用予定ID
    pub conflicting_usage_id: UsageId,
}

impl ResourceConflictError {
    pub fn new(resource_description: impl Into<String>, conflicting_usage_id: UsageId) -> Self {
        Self {
            resource_description: resource_description.into(),
            conflicting_usage_id,
        }
    }
}

impl fmt::Display for ResourceConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "リソース競合: {} (競合する予約ID: {})",
            self.resource_description,
            self.conflicting_usage_id.as_str()
        )
    }
}

impl std::error::Error for ResourceConflictError {}

impl DomainError for ResourceConflictError {}

/// 競合チェックで発生するエラー
#[derive(Debug)]
pub enum ConflictCheckError {
    /// リソース競合
    Conflict(ResourceConflictError),
    /// リポジトリエラー
    Repository(RepositoryError),
}

impl fmt::Display for ConflictCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictCheckError::Conflict(e) => write!(f, "{}", e),
            ConflictCheckError::Repository(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ConflictCheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConflictCheckError::Conflict(e) => Some(e),
            ConflictCheckError::Repository(e) => Some(e),
        }
    }
}

impl From<RepositoryError> for ConflictCheckError {
    fn from(e: RepositoryError) -> Self {
        ConflictCheckError::Repository(e)
    }
}

impl DomainError for ConflictCheckError {}
