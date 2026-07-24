//! リソース使用ドメインサービスのエラー

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::errors::DomainError;
use crate::domain::ports::repositories::RepositoryError;
use std::fmt;

/// リソース競合エラー
///
/// 競合したリソースと既存の使用予定そのものを保持する。
/// メッセージ文言はインターフェース層が設定に基づいて組み立てられるよう、
/// ここでは構造化データのまま渡す。
/// `existing_usage`は`ResourceUsage`をそのまま持たせると`Result`の
/// エラー型が肥大化する（`clippy::result_large_err`）ため`Box`で保持する。
#[derive(Debug)]
pub struct ResourceConflictError {
    /// 競合しているリソース
    pub resource: Resource,
    /// 競合している既存の使用予定
    pub existing_usage: Box<ResourceUsage>,
}

impl ResourceConflictError {
    pub fn new(resource: Resource, existing_usage: ResourceUsage) -> Self {
        Self {
            resource,
            existing_usage: Box::new(existing_usage),
        }
    }
}

impl fmt::Display for ResourceConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "リソース競合: {} (競合する予約ID: {})",
            self.resource,
            self.existing_usage.id().as_str()
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
