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

impl Default for SlackSender {
    fn default() -> Self {
        Self::new()
    }
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
                    "🔔 新規予約\n👤 {}\n\n📅 期間\n{}\n\n💻 予約GPU\n{}",
                    user_display, time_period, resources
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "🔄 予約更新\n👤 {}\n\n📅 期間\n{}\n\n💻 予約GPU\n{}",
                    user_display, time_period, resources
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ 予約削除\n👤 {}\n\n📅 期間\n{}\n\n💻 予約GPU\n{}",
                    user_display, time_period, resources
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
        if let Some(identity) = identity_link
            && let Some(slack_identity) = identity.get_identity_for_system(&ExternalSystem::Slack)
        {
            return format!("<@{}>", slack_identity.user_id());
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
            .map_err(|e| NotificationError::SendFailure(format!("Slack送信失敗: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{Gpu, Resource, TimePeriod, UsageId},
    };
    use crate::domain::common::EmailAddress;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_format_created_message_with_gpu() {
        let sender = SlackSender::new();
        let email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let gpu = Gpu::new("Thalys".to_string(), 0, "A100".to_string());
        let resources = vec![Resource::Gpu(gpu)];
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_period = TimePeriod::new(start, end).unwrap();
        let usage = ResourceUsage::new(
            UsageId::new("test-id".to_string()),
            email,
            time_period,
            resources,
            None,
        )
        .unwrap();

        let event = NotificationEvent::ResourceUsageCreated(usage);
        let context = NotificationContext {
            event: &event,
            identity_link: None,
        };

        let message = sender.format_message(&context);

        // メッセージに絵文字が含まれることを確認
        assert!(message.contains("🔔"));
        assert!(message.contains("👤"));
        assert!(message.contains("📅"));
        assert!(message.contains("💻"));
        // メッセージが構造化されていることを確認
        assert!(message.contains("新規予約"));
        assert!(message.contains("期間"));
        assert!(message.contains("予約GPU"));
        assert!(message.contains("Thalys / A100 / GPU:0"));
    }

    #[test]
    fn test_format_updated_message_with_room() {
        let sender = SlackSender::new();
        let email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let resources = vec![Resource::Room {
            name: "会議室A".to_string(),
        }];
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_period = TimePeriod::new(start, end).unwrap();
        let usage = ResourceUsage::new(
            UsageId::new("test-id".to_string()),
            email,
            time_period,
            resources,
            None,
        )
        .unwrap();

        let event = NotificationEvent::ResourceUsageUpdated(usage);
        let context = NotificationContext {
            event: &event,
            identity_link: None,
        };

        let message = sender.format_message(&context);

        // メッセージに絵文字が含まれることを確認
        assert!(message.contains("🔄"));
        assert!(message.contains("📅"));
        assert!(message.contains("💻"));
        // メッセージが構造化されていることを確認
        assert!(message.contains("予約更新"));
        assert!(message.contains("会議室A"));
    }

    #[test]
    fn test_format_deleted_message() {
        let sender = SlackSender::new();
        let email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let gpu = Gpu::new("Thalys".to_string(), 1, "A100".to_string());
        let resources = vec![Resource::Gpu(gpu)];
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_period = TimePeriod::new(start, end).unwrap();
        let usage = ResourceUsage::new(
            UsageId::new("test-id".to_string()),
            email,
            time_period,
            resources,
            None,
        )
        .unwrap();

        let event = NotificationEvent::ResourceUsageDeleted(usage);
        let context = NotificationContext {
            event: &event,
            identity_link: None,
        };

        let message = sender.format_message(&context);

        // メッセージに絵文字が含まれることを確認
        assert!(message.contains("🗑️"));
        assert!(message.contains("📅"));
        assert!(message.contains("💻"));
        // メッセージが構造化されていることを確認
        assert!(message.contains("予約削除"));
    }
}
