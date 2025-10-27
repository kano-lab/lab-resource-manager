use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;

/// 通知メッセージを送信する機能を提供するtrait
///
/// このtraitは具体的な送信手段（Slack, Discord, Email, Mock等）に対する
/// 共通インターフェースを定義します。
///
/// # Associated Type
/// * `Config` - 送信に必要な設定の型（webhook URL, email address等）
#[async_trait]
pub trait Sender: Send + Sync {
    type Config: ?Sized;

    /// 指定された設定を使ってメッセージを送信
    ///
    /// # Arguments
    /// * `config` - 送信先の設定
    /// * `message` - 送信するメッセージ
    async fn send(&self, config: &Self::Config, message: &str) -> Result<(), NotificationError>;
}
