use crate::domain::aggregates::identity_link::errors::IdentityLinkError;
use crate::domain::aggregates::resource_usage::errors::ResourceUsageError;
use crate::domain::ports::{
    notifier::NotificationError, repositories::RepositoryError,
    resource_collection_access::ResourceCollectionAccessError,
};
use crate::domain::services::resource_usage::errors::ResourceConflictError;
use std::fmt;

/// Application層で発生するエラーの列挙型
///
/// インフラストラクチャ層、ドメイン層、およびユースケース固有のエラーをラップする。
#[derive(Debug)]
pub enum ApplicationError {
    /// リポジトリ操作中に発生したエラー
    Repository(RepositoryError),
    /// 通知送信中に発生したエラー
    Notification(NotificationError),
    /// リソースコレクションへのアクセス中に発生したエラー
    ResourceCollectionAccess(ResourceCollectionAccessError),

    /// リソース使用に関するドメインエラー
    ResourceUsage(ResourceUsageError),
    /// ID紐付けに関するドメインエラー
    IdentityLink(IdentityLinkError),

    /// 外部システムが既に紐付けられている
    ExternalSystemAlreadyLinked {
        /// 紐付けられているメールアドレス
        email: String,
        /// 既に紐付けられている外部システム名
        external_system: String,
    },

    /// リソースの競合エラー
    ResourceConflict {
        /// 競合しているリソースの説明
        resource_description: String,
        /// 競合している既存の使用予定ID
        conflicting_usage_id: String,
    },

    /// 認可エラー（権限不足）
    Unauthorized(String),
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
            ApplicationError::ResourceConflict {
                resource_description,
                conflicting_usage_id,
            } => {
                write!(
                    f,
                    "リソース {} は既に使用予定 {} で使用されています",
                    resource_description, conflicting_usage_id
                )
            }
            ApplicationError::Unauthorized(msg) => {
                write!(f, "権限不足: {}", msg)
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
            ApplicationError::ResourceConflict { .. } => None,
            ApplicationError::Unauthorized(_) => None,
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

impl From<ResourceConflictError> for ApplicationError {
    fn from(e: ResourceConflictError) -> Self {
        ApplicationError::ResourceConflict {
            resource_description: e.resource_description,
            conflicting_usage_id: e.conflicting_usage_id.as_str().to_string(),
        }
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
