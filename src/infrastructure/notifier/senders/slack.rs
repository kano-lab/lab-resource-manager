//! Slack通知送信モジュール

use async_trait::async_trait;
use serde_json::json;
use slack_morphism::prelude::*;
use tracing::error;

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::reservation::Reservation;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::notifier::senders::sender::{NotificationContext, Sender};
use crate::infrastructure::notifier::template_renderer::{TemplateRenderer, render_event};
use crate::interface::slack::constants::{
    ACTION_CANCEL_SLOT, ACTION_END_SLOT_EARLY, ACTION_UPDATE_SLOT,
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
        subject_id: &str,
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
            subject_id,
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

        let user_display = Self::format_user(context.event.holder_email(), context.identity_link);
        render_event(&renderer, context.event, &user_display)
    }

    /// メッセージブロックを構築（イベントに応じてボタンを追加）
    fn build_message_blocks(message: &str, context: &NotificationContext) -> Vec<SlackBlock> {
        // ボタンは予約枠に対する操作（更新・今で終了・取り消し）なので、対象の予約枠が
        // ちょうど1つに決まり、まだ残っている場合にだけ付ける。単発予約は予約枠を1つだけ
        // 持つので、予約のIDがそのまま予約枠のIDになる。定期予約全体を指す通知からは
        // 対象の予約枠を決められないため付けない
        let actionable_slot_id = match context.event {
            NotificationEvent::ReservationCreated(Reservation::OneTime(reservation))
            | NotificationEvent::ReservationUpdated(Reservation::OneTime(reservation)) => {
                Some(reservation.id().as_str())
            }
            NotificationEvent::SlotUpdated(slot) => Some(slot.id().as_str()),
            NotificationEvent::ReservationCreated(Reservation::Recurring(_))
            | NotificationEvent::ReservationUpdated(Reservation::Recurring(_))
            | NotificationEvent::ReservationCancelled(_)
            | NotificationEvent::SlotCancelled(_) => None,
        };

        let blocks_json = if let Some(slot_id) = actionable_slot_id {
            tracing::debug!(slot_id = %slot_id, "attached action buttons to the notification");
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
                            "action_id": ACTION_UPDATE_SLOT,
                            "value": slot_id
                        },
                        {
                            "type": "button",
                            "text": {
                                "type": "plain_text",
                                "text": "⏹️ 今で終了"
                            },
                            "action_id": ACTION_END_SLOT_EARLY,
                            "value": slot_id
                        },
                        {
                            "type": "button",
                            "text": {
                                "type": "plain_text",
                                "text": "❌ 取り消す"
                            },
                            "style": "danger",
                            "action_id": ACTION_CANCEL_SLOT,
                            "value": slot_id
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

        let subject_id = context.event.subject_id().to_string();

        // Bot Token方式
        self.send_via_bot_token(
            &config.bot_token,
            &config.channel_id,
            message,
            blocks,
            &subject_id,
        )
        .await
    }
}
