//! Slack通知送信モジュール

use async_trait::async_trait;
use serde_json::json;
use slack_morphism::prelude::*;
use tracing::error;

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::notifier::senders::sender::{NotificationContext, Sender};
use crate::infrastructure::notifier::template_renderer::TemplateRenderer;
use crate::interface::slack::constants::{ACTION_CANCEL_RESERVATION, ACTION_EDIT_RESERVATION};

/// Slack通知設定
pub struct SlackNotificationConfig {
    pub bot_token: String,
    pub channel_id: String,
}

/// Slack経由でメッセージを送信する（Bot Token方式）
pub struct SlackSender {
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
            slack_client: SlackClient::new(
                SlackClientHyperConnector::new()
                    .expect("Failed to initialize Slack HTTP connector"),
            ),
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

    /// ユーザー表示名をフォーマット（Slackメンション or メールアドレス）
    fn format_user(
        email: &EmailAddress,
        identity_link: Option<&crate::domain::aggregates::identity_link::entity::IdentityLink>,
    ) -> String {
        if let Some(identity) = identity_link
            && let Some(slack_identity) = identity.get_identity_for_system(&ExternalSystem::Slack)
        {
            return format!("<@{}>", slack_identity.user_id());
        }
        email.as_str().to_string()
    }

    /// イベントからSlack用のメッセージを構築（テンプレートレンダラー使用）
    fn format_message(context: &NotificationContext) -> String {
        let usage = Self::extract_usage_from_event(context.event);
        let user_display = Self::format_user(usage.owner_email(), context.identity_link);

        let renderer = TemplateRenderer::new(
            &context.customization.templates,
            &context.customization.format,
            context.timezone,
        );

        match context.event {
            NotificationEvent::ResourceUsageCreated(_) => {
                renderer.render_created(usage, &user_display)
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                renderer.render_updated(usage, &user_display)
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                renderer.render_deleted(usage, &user_display)
            }
        }
    }

    /// イベントからResourceUsageを抽出
    fn extract_usage_from_event(event: &NotificationEvent) -> &ResourceUsage {
        match event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        }
    }

    /// メッセージブロックを構築（イベントに応じてボタンを追加）
    fn build_message_blocks(message: &str, context: &NotificationContext) -> Vec<SlackBlock> {
        let usage = Self::extract_usage_from_event(context.event);
        let usage_id = usage.id().as_str();
        tracing::info!("🔔 通知ボタン作成: usage_id={}", usage_id);

        // Deleted イベントの場合はボタンなし
        let should_add_buttons = matches!(
            context.event,
            NotificationEvent::ResourceUsageCreated(_) | NotificationEvent::ResourceUsageUpdated(_)
        );

        let blocks_json = if should_add_buttons {
            // ボタン付きブロック
            json!([
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": message
                    }
                },
                {
                    "type": "actions",
                    "elements": [
                        {
                            "type": "button",
                            "text": {
                                "type": "plain_text",
                                "text": "🔄 更新"
                            },
                            "style": "primary",
                            "action_id": ACTION_EDIT_RESERVATION,
                            "value": usage_id
                        },
                        {
                            "type": "button",
                            "text": {
                                "type": "plain_text",
                                "text": "❌ キャンセル"
                            },
                            "style": "danger",
                            "action_id": ACTION_CANCEL_RESERVATION,
                            "value": usage_id
                        }
                    ]
                }
            ])
        } else {
            // シンプルなブロック（Deletedイベント用）
            json!([
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": message
                    }
                }
            ])
        };

        serde_json::from_value(blocks_json).unwrap_or_else(|e| {
            error!("Failed to deserialize Slack blocks: {}", e);
            vec![]
        })
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
        let message = Self::format_message(&context);
        let blocks = Self::build_message_blocks(&message, &context);

        // Bot Token方式
        self.send_via_bot_token(&config.bot_token, &config.channel_id, message, blocks)
            .await
    }
}
