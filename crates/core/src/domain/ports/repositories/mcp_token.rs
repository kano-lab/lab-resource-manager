use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::RepositoryError;
use async_trait::async_trait;

/// MCPアクセストークンのリポジトリポート
///
/// トークンは1メールアドレスにつき常に最大1つ。`issue_token`を再度呼ぶと
/// 既存のトークンは失効し、新しいトークンに置き換わる。
#[async_trait]
pub trait McpTokenRepository: Send + Sync {
    /// 新しいトークンを発行する。同じメールアドレスの既存トークンは失効する。
    async fn issue_token(&self, email: &EmailAddress) -> Result<String, RepositoryError>;

    /// トークンからメールアドレスを解決する。無効なトークンは`None`。
    async fn resolve(&self, token: &str) -> Result<Option<EmailAddress>, RepositoryError>;
}
