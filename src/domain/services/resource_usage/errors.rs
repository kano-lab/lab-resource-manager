//! リソース使用ドメインサービスのエラー

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::errors::DomainError;
use crate::domain::ports::repositories::RepositoryError;
use std::fmt;

/// リソース競合エラー（リクエストしたリソース1件分）
///
/// ひとつの既存予約に対して1件で表す。予約は複数のリソースを押さえうるため、
/// その予約と競合したリソースをまとめて保持する。リソースごとに分けて返すと、
/// 受け取った側が予約単位へまとめ直す必要が生じる。
///
/// メッセージ文言はインターフェース層が設定に基づいて組み立てられるよう、
/// ここでは構造化データのまま渡す。
#[derive(Debug)]
pub struct ResourceConflictError {
    /// この予約と競合したリソース
    pub resources: Vec<Resource>,
    /// 競合している既存の使用予定
    pub existing_usage: ResourceUsage,
}

impl ResourceConflictError {
    pub fn new(resources: Vec<Resource>, existing_usage: ResourceUsage) -> Self {
        Self {
            resources,
            existing_usage,
        }
    }
}

impl fmt::Display for ResourceConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resources = self
            .resources
            .iter()
            .map(|resource| resource.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "リソース競合: {} (競合する予約ID: {})",
            resources,
            self.existing_usage.id().as_str()
        )
    }
}

impl std::error::Error for ResourceConflictError {}

impl DomainError for ResourceConflictError {}

/// 競合チェックで発生するエラー
#[derive(Debug)]
pub enum ConflictCheckError {
    /// リソース競合（リクエストしたリソースのうち競合した全件）
    ///
    /// `Vec`で保持しているため、`Result`のエラー型が肥大化する
    /// （`clippy::result_large_err`）ことはない。
    Conflict(Vec<ResourceConflictError>),
    /// リポジトリエラー
    Repository(RepositoryError),
}

impl fmt::Display for ConflictCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictCheckError::Conflict(conflicts) => {
                let messages: Vec<String> = conflicts.iter().map(ToString::to_string).collect();
                write!(f, "{}", messages.join("; "))
            }
            ConflictCheckError::Repository(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ConflictCheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConflictCheckError::Conflict(conflicts) => conflicts
                .first()
                .map(|c| c as &(dyn std::error::Error + 'static)),
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
