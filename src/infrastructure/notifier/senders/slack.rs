//! Slack通知送信モジュール

use async_trait::async_trait;
use serde_json::json;
use slack_morphism::prelude::*;

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::notifier::senders::sender::{NotificationContext, Sender};

/// Slack通知設定
pub struct SlackNotificationConfig {
    pub bot_token: Option<String>,
    pub channel_id: Option<String>,
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
            SlackMessageContent::new().with_text(message).with_blocks(blocks),
        );

        session
            .chat_post_message(&post_chat_req)
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack API送信失敗: {}", e)))?;

        Ok(())
    }

    /// リソースタイプに応じたラベルを生成
    fn get_resource_label(resources: &[Resource]) -> &'static str {
        if resources.is_empty() {
            return "📦 予約リソース";
        }

        let has_gpu = resources.iter().any(|r| matches!(r, Resource::Gpu(_)));
        let has_room = resources.iter().any(|r| matches!(r, Resource::Room { .. }));

        match (has_gpu, has_room) {
            (true, false) => "💻 予約GPU",
            (false, true) => "🏢 予約部屋",
            _ => "📦 予約リソース", // 混在または不明
        }
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

    /// イベントからSlack用のメッセージを構築
    fn format_message(context: &NotificationContext) -> String {
        let usage = Self::extract_usage_from_event(context.event);
        let user_display = Self::format_user(usage.owner_email(), context.identity_link);
        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period(), context.timezone);
        let resource_label = Self::get_resource_label(usage.resources());

        match context.event {
            NotificationEvent::ResourceUsageCreated(_) => {
                format!(
                    "🔔 新規予約\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "🔄 予約更新\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ 予約削除\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
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

    /// シンプルなメッセージブロックを構築
    fn build_message_blocks(message: &str) -> Vec<SlackBlock> {
        let blocks_json = json!([
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": message
                }
            }
        ]);

        serde_json::from_value(blocks_json).unwrap_or_else(|_| vec![])
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
        let blocks = Self::build_message_blocks(&message);

        // Bot Token方式
        if let (Some(bot_token), Some(channel_id)) = (&config.bot_token, &config.channel_id) {
            self.send_via_bot_token(bot_token, channel_id, message, blocks)
                .await?;
        } else {
            return Err(NotificationError::SendFailure(
                "bot_token と channel_id が設定されていません".to_string(),
            ));
        }

        Ok(())
    }
}
