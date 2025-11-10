use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use slack_morphism::prelude::*;

use super::sender::{NotificationContext, Sender};

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

    /// リソースタイプに応じたラベルを生成
    fn get_resource_label(
        &self,
        resources: &[crate::domain::aggregates::resource_usage::value_objects::Resource],
    ) -> &'static str {
        use crate::domain::aggregates::resource_usage::value_objects::Resource;

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

    /// イベントからSlack用のメッセージを構築
    fn format_message(&self, context: &NotificationContext) -> String {
        let usage = match context.event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        };

        let user_display = self.format_user(usage.owner_email(), context.identity_link);
        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period(), context.timezone);
        let resource_label = self.get_resource_label(usage.resources());

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
    type Config = SlackNotificationConfig;

    async fn send(
        &self,
        config: &SlackNotificationConfig,
        context: NotificationContext<'_>,
    ) -> Result<(), NotificationError> {
        let message = self.format_message(&context);
        let usage_id = match context.event {
            NotificationEvent::ResourceUsageCreated(u) => u.id().as_str(),
            NotificationEvent::ResourceUsageUpdated(u) => u.id().as_str(),
            NotificationEvent::ResourceUsageDeleted(u) => u.id().as_str(),
        };

        // Block Kit形式でボタン付きメッセージを構築（JSON形式）
        let blocks_json = json!([
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
                        "action_id": "edit_reservation",
                        "value": usage_id
                    },
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "❌ キャンセル"
                        },
                        "style": "danger",
                        "action_id": "cancel_reservation",
                        "value": usage_id
                    }
                ]
            }
        ]);

        // bot_tokenがあればAPI経由、なければWebhook経由
        if let (Some(bot_token), Some(channel_id)) = (&config.bot_token, &config.channel_id) {
            // Bot Token方式（インタラクティブボタン対応）
            let token = SlackApiToken::new(bot_token.clone().into());
            let session = self.slack_client.open_session(&token);

            // blocksをSlackBlock形式にデシリアライズ
            let blocks: Vec<SlackBlock> =
                serde_json::from_value(blocks_json.clone()).unwrap_or_else(|_| vec![]);

            let post_chat_req = SlackApiChatPostMessageRequest::new(
                channel_id.clone().into(),
                SlackMessageContent::new()
                    .with_text(message.clone())
                    .with_blocks(blocks),
            );

            session
                .chat_post_message(&post_chat_req)
                .await
                .map_err(|e| NotificationError::SendFailure(format!("Slack API送信失敗: {}", e)))?;
        } else if let Some(webhook_url) = &config.webhook_url {
            // Webhook方式（レガシー、ボタンは動作しない）
            let payload = json!({
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
        } else {
            return Err(NotificationError::SendFailure(
                "bot_token+channel_id または webhook_url が設定されていません".to_string(),
            ));
        }

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
            timezone: None,
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
            timezone: None,
        };

        let message = sender.format_message(&context);

        // メッセージに絵文字が含まれることを確認
        assert!(message.contains("🔄"));
        assert!(message.contains("📅"));
        assert!(message.contains("🏢"));
        // メッセージが構造化されていることを確認
        assert!(message.contains("予約更新"));
        assert!(message.contains("予約部屋"));
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
            timezone: None,
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
