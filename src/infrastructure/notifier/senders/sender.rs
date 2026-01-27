use crate::domain::aggregates::identity_link::entity::IdentityLink;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::config::NotificationCustomization;
use async_trait::async_trait;

/// 通知送信に必要なコンテキスト情報
pub struct NotificationContext<'a> {
    /// 通知イベント
    pub event: &'a NotificationEvent,
    /// ユーザーのID紐付け情報（オプション）
    pub identity_link: Option<&'a IdentityLink>,
    /// タイムゾーン（オプション）
    pub timezone: Option<&'a str>,
    /// カスタマイズ設定
    pub customization: NotificationCustomization,
}

/// 通知メッセージを送信する機能を提供するtrait
///
/// このtraitは具体的な送信手段（Slack, Discord, Email, Mock等）に対する
/// 共通インターフェースを定義します。
///
/// # Associated Type
/// * `Config` - 送信に必要な設定の型（bot token, channel ID, email address等）
#[async_trait]
pub trait Sender: Send + Sync {
    /// 送信に必要な設定の型
    type Config: ?Sized;

    /// 指定された設定を使ってメッセージを送信
    ///
    /// 各Senderは受け取ったイベントを自身のフォーマットでメッセージ化し、送信する
    ///
    /// # Arguments
    /// * `config` - 送信先の設定
    /// * `context` - 送信コンテキスト（イベント、ユーザー識別情報等）
    async fn send(
        &self,
        config: &Self::Config,
        context: NotificationContext<'_>,
    ) -> Result<(), NotificationError>;
}
