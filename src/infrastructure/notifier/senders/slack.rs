//! Slack通知送信モジュール

use async_trait::async_trait;
use serde_json::json;
use slack_morphism::prelude::*;
use tracing::error;

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::notifier::senders::sender::{NotificationContext, Sender};
use crate::infrastructure::notifier::template_renderer::{TemplateRenderer, render_event};
use crate::interface::slack::constants::{
    ACTION_CANCEL_RESERVATION, ACTION_EDIT_RESERVATION, ACTION_RELEASE_RESERVATION_EARLY,
};

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
        usage_id: &str,
    ) -> Result<(), NotificationError> {
        let token = SlackApiToken::new(bot_token.into());
        let session = self.slack_client.open_session(&token);

        let post_chat_req = SlackApiChatPostMessageRequest::new(
            channel_id.into(),
            SlackMessageContent::new()
                .with_text(message)
                .with_blocks(blocks),
        );

        let sent = session
            .chat_post_message(&post_chat_req)
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack API送信失敗: {}", e)))?;

        // 受け取った側から「この通知はおかしい」と言われたとき、channelとtsの組で
        // 当該メッセージを特定できる
        tracing::info!(
            channel = %sent.channel,
            ts = %sent.ts,
            usage_id,
            "sent a reservation notification"
        );

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
        let renderer = TemplateRenderer::new(
            &context.customization.templates,
            &context.customization.format,
            context.timezone,
        );

        let user_display = Self::format_user(context.event.owner_email(), context.identity_link);
        render_event(&renderer, context.event, &user_display)
    }

    /// メッセージブロックを構築（イベントに応じてボタンを追加）
    fn build_message_blocks(message: &str, context: &NotificationContext) -> Vec<SlackBlock> {
        // 操作できる予約が残っている場合にだけボタンを付ける。削除済みの予約には
        // 操作する対象が無い。定期予約のボタンも付けない。更新・早期終了・キャンセルは
        // 回ごとの操作であり、定期予約全体を指す通知からは対象の回を決められない
        let actionable_usage_id = match context.event {
            NotificationEvent::ResourceUsageCreated(usage)
            | NotificationEvent::ResourceUsageUpdated(usage) => Some(usage.id().as_str()),
            NotificationEvent::ResourceUsageDeleted(_)
            | NotificationEvent::RecurringReservationCreated(_)
            | NotificationEvent::RecurringReservationUpdated(_)
            | NotificationEvent::RecurringReservationCancelled(_) => None,
        };

        let blocks_json = if let Some(usage_id) = actionable_usage_id {
            tracing::debug!(usage_id = %usage_id, "attached action buttons to the notification");
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
                                "text": "⏹️ 今で終了"
                            },
                            "action_id": ACTION_RELEASE_RESERVATION_EARLY,
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
            // シンプルなブロック（操作する対象が無いイベント用）
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
            error!(error = %e, "building the notification message blocks failed");
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

        let usage_id = context.event.subject_id().to_string();

        // Bot Token方式
        self.send_via_bot_token(
            &config.bot_token,
            &config.channel_id,
            message,
            blocks,
            &usage_id,
        )
        .await
    }
}
