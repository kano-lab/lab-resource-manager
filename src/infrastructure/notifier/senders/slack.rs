use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use super::sender::{NotificationContext, Sender};

/// Slack Webhook経由でメッセージを送信する
pub struct SlackSender {
    client: Client,
}

impl SlackSender {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// イベントからSlack用のメッセージを構築
    fn format_message(&self, context: &NotificationContext) -> String {
        let usage = match context.event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        };

        let user_display = self.format_user(usage.owner_email(), context.identity_link);
        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period());

        match context.event {
            NotificationEvent::ResourceUsageCreated(_) => {
                format!(
                    "🔔 新規予約\n{} が {} を予約しました\n期間: {}",
                    user_display, resources, time_period
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "🔄 予約更新\n{} が {} の予約を変更しました\n期間: {}",
                    user_display, resources, time_period
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ 予約削除\n{} が {} の予約をキャンセルしました\n期間: {}",
                    user_display, resources, time_period
                )
            }
        }
    }

    /// ユーザー表示名をフォーマット（Slackメンション or メールアドレス）
    fn format_user(
        &self,
        email: &crate::domain::common::EmailAddress,
        identity_link: Option<&crate::domain::aggregates::identity_link::entity::IdentityLink>,
    ) -> String {
        if let Some(identity) = identity_link {
            if let Some(slack_identity) = identity.get_identity_for_system(&ExternalSystem::Slack)
            {
                return format!("<@{}>", slack_identity.user_id());
            }
        }
        email.as_str().to_string()
    }
}

#[async_trait]
impl Sender for SlackSender {
    type Config = str;

    async fn send(
        &self,
        webhook_url: &str,
        context: NotificationContext<'_>,
    ) -> Result<(), NotificationError> {
        let message = self.format_message(&context);

        let payload = json!({
            "text": message
        });

        self.client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError {
                message: format!("Slack送信失敗: {}", e),
            })?;

        Ok(())
    }
}
