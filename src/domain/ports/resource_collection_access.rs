use crate::domain::common::EmailAddress;
use async_trait::async_trait;
use std::fmt;

/// リソースコレクションアクセスのエラー型
#[derive(Debug, Clone)]
pub enum ResourceCollectionAccessError {
    /// 認証エラー
    AuthenticationError(String),
    /// APIエラー
    ApiError(String),
    /// リソースコレクションが見つからない
    CollectionNotFound(String),
    /// 権限エラー
    PermissionDenied(String),
    /// その他のエラー
    Unknown(String),
}

impl fmt::Display for ResourceCollectionAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationError(msg) => write!(f, "Authentication error: {}", msg),
            Self::ApiError(msg) => write!(f, "API error: {}", msg),
            Self::CollectionNotFound(id) => write!(f, "Resource collection not found: {}", id),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for ResourceCollectionAccessError {}

/// リソースコレクションアクセスサービスのインターフェース
///
/// ResourceUsageを管理するコレクション（例：Googleカレンダー）へのアクセス権限を管理する。
/// このサービスはユーザーにリソース予約権限を付与・剥奪する責務を持つ。
#[async_trait]
pub trait ResourceCollectionAccessService: Send + Sync {
    /// 指定したメールアドレスにリソースコレクションへのアクセス権を付与する
    ///
    /// # 引数
    /// * `collection_id` - リソースコレクションのID（実装により異なる）
    /// * `email` - アクセス権を付与するメールアドレス
    ///
    /// # エラー
    /// - リソースコレクションが見つからない場合
    /// - API通信エラー
    /// - 権限不足
    async fn grant_access(
        &self,
        collection_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError>;

    /// リソースコレクションへのアクセス権を解除する
    ///
    /// # 引数
    /// * `collection_id` - リソースコレクションのID
    /// * `email` - アクセス権を解除するメールアドレス
    async fn revoke_access(
        &self,
        collection_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError>;
}
