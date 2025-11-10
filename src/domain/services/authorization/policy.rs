use crate::domain::common::EmailAddress;
use std::fmt;

/// 認可エラー
#[derive(Debug, Clone, PartialEq)]
pub enum AuthorizationError {
    /// 権限不足
    Forbidden {
        actor: EmailAddress,
        action: String,
        resource: String,
    },
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthorizationError::Forbidden {
                actor,
                action,
                resource,
            } => write!(
                f,
                "ユーザー {} には {} を {} する権限がありません",
                actor.as_str(),
                resource,
                action
            ),
        }
    }
}

impl std::error::Error for AuthorizationError {}

/// 認可ポリシートレイト
///
/// Actor-Action-Resourceモデルに基づいた認可を提供
pub trait AuthorizationPolicy<T> {
    /// 更新権限をチェック
    fn authorize_update(
        &self,
        actor: &EmailAddress,
        resource: &T,
    ) -> Result<(), AuthorizationError>;

    /// 削除権限をチェック
    fn authorize_delete(
        &self,
        actor: &EmailAddress,
        resource: &T,
    ) -> Result<(), AuthorizationError>;

    /// 読み取り権限をチェック（オプション）
    fn authorize_read(
        &self,
        _actor: &EmailAddress,
        _resource: &T,
    ) -> Result<(), AuthorizationError> {
        // デフォルトでは全員が読み取り可能
        Ok(())
    }
}
