use crate::domain::aggregates::identity_link::errors::IdentityLinkError;
use crate::domain::aggregates::resource_usage::errors::ResourceUsageError;
use crate::domain::ports::{
    notifier::NotificationError, repositories::RepositoryError,
    resource_collection_access::ResourceCollectionAccessError,
    resource_usage_observer::ObservationError,
};
use crate::domain::services::resource_usage::errors::{ConflictCheckError, ResourceConflictError};
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
    /// 実サーバーの利用状況観測中に発生したエラー
    Observation(ObservationError),

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
    ///
    /// リクエストしたリソースのうち競合した全件を構造化データとして保持する。
    /// インターフェース層が設定（テンプレート・フォーマットスタイル）に基づいて
    /// ユーザー向けメッセージを組み立てられるようにするため。
    /// `Vec`で保持しているため`ApplicationError`全体のサイズは肥大化しない
    /// （`clippy::result_large_err`）。
    ResourceConflict {
        /// 競合した全件（リクエストしたリソースごとに最大1件）
        conflicts: Vec<ResourceConflictError>,
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
            ApplicationError::Observation(e) => write!(f, "利用状況観測エラー: {}", e),
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
            ApplicationError::ResourceConflict { conflicts } => {
                let messages: Vec<String> = conflicts
                    .iter()
                    .map(|c| {
                        format!(
                            "リソース {} は既に使用予定 {} で使用されています",
                            c.resources
                                .iter()
                                .map(|resource| resource.to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                            c.existing_usage.id().as_str()
                        )
                    })
                    .collect();
                write!(f, "{}", messages.join("; "))
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
            ApplicationError::Observation(e) => Some(e),
            ApplicationError::ResourceUsage(e) => Some(e),
            ApplicationError::IdentityLink(e) => Some(e),
            ApplicationError::ExternalSystemAlreadyLinked { .. } => None,
            ApplicationError::ResourceConflict { conflicts } => conflicts
                .first()
                .map(|c| c as &(dyn std::error::Error + 'static)),
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

impl From<ConflictCheckError> for ApplicationError {
    fn from(e: ConflictCheckError) -> Self {
        match e {
            ConflictCheckError::Conflict(conflicts) => {
                ApplicationError::ResourceConflict { conflicts }
            }
            ConflictCheckError::Repository(repo_err) => ApplicationError::Repository(repo_err),
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

impl From<ObservationError> for ApplicationError {
    fn from(e: ObservationError) -> Self {
        ApplicationError::Observation(e)
    }
}
