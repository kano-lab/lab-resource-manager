//! Slack通知送信モジュール

mod block_builder;
mod formatter;

use async_trait::async_trait;
use reqwest::Client;
use slack_morphism::prelude::*;

use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::notifier::senders::sender::{NotificationContext, Sender};

pub use block_builder::SlackBlockBuilder;
pub use formatter::SlackMessageFormatter;

/// Slack通知設定
pub struct SlackNotificationConfig {
    pub bot_token: Option<String>,
    pub channel_id: Option<String>,
    pub webhook_url: Option<String>,
}

/// Slack経由でメッセージを送信する（Bot Token or Webhook）
pub struct SlackSender {
    client: Client,
    slack_client: SlackClient<SlackClientHyperHttpsConnector>,
}

impl Default for SlackSender {
    fn default() -> Self {
        Self::new()
    }
}

impl SlackSender {
    /// 新しいSlackSenderを作成
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            slack_client: SlackClient::new(SlackClientHyperConnector::new().unwrap()),
        }
    }

    /// Bot Token方式でメッセージを送信
    async fn send_via_bot_token(
        &self,
        bot_token: &str,
        channel_id: &str,
        message: String,
        blocks: Vec<SlackBlock>,
    ) -> Result<(), NotificationError> {
        let token = SlackApiToken::new(bot_token.into());
        let session = self.slack_client.open_session(&token);

        let post_chat_req = SlackApiChatPostMessageRequest::new(
            channel_id.into(),
            SlackMessageContent::new()
                .with_text(message)
                .with_blocks(blocks),
        );

        session
            .chat_post_message(&post_chat_req)
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack API送信失敗: {}", e)))?;

        Ok(())
    }

    /// Webhook方式でメッセージを送信（レガシー）
    async fn send_via_webhook(
        &self,
        webhook_url: &str,
        message: String,
        blocks_json: serde_json::Value,
    ) -> Result<(), NotificationError> {
        let payload = serde_json::json!({
            "text": message,  // フォールバック用
            "blocks": blocks_json
        });

        self.client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                NotificationError::SendFailure(format!("Slack Webhook送信失敗: {}", e))
            })?;

        Ok(())
    }
}

#[async_trait]
impl Sender for SlackSender {
    type Config = SlackNotificationConfig;

    async fn send(
        &self,
        config: &SlackNotificationConfig,
        context: NotificationContext<'_>,
    ) -> Result<(), NotificationError> {
        // メッセージとブロックを構築
        let message = SlackMessageFormatter::format_message(&context);
        let usage_id = match context.event {
            crate::domain::ports::notifier::NotificationEvent::ResourceUsageCreated(u) => {
                u.id().as_str()
            }
            crate::domain::ports::notifier::NotificationEvent::ResourceUsageUpdated(u) => {
                u.id().as_str()
            }
            crate::domain::ports::notifier::NotificationEvent::ResourceUsageDeleted(u) => {
                u.id().as_str()
            }
        };

        let blocks_json = SlackBlockBuilder::build_message_with_buttons(&message, usage_id);

        // bot_tokenがあればAPI経由、なければWebhook経由
        if let (Some(bot_token), Some(channel_id)) = (&config.bot_token, &config.channel_id) {
            // Bot Token方式（インタラクティブボタン対応）
            let blocks = SlackBlockBuilder::json_to_slack_blocks(blocks_json);
            self.send_via_bot_token(bot_token, channel_id, message, blocks)
                .await?;
        } else if let Some(webhook_url) = &config.webhook_url {
            // Webhook方式（レガシー、ボタンは動作しない）
            self.send_via_webhook(webhook_url, message, blocks_json)
                .await?;
        } else {
            return Err(NotificationError::SendFailure(
                "bot_token+channel_id または webhook_url が設定されていません".to_string(),
            ));
        }

        Ok(())
    }
}
