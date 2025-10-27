use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::ports::notifier::{NotificationError, NotificationEvent, Notifier};
use crate::infrastructure::config::{NotificationConfig, ResourceConfig};
use async_trait::async_trait;
use std::collections::HashSet;

use super::senders::{MockSender, SlackSender, sender::Sender};

/// 複数の通知手段をオーケストレートし、リソースに基づいて適切な通知先にルーティングする
///
/// 各種Sender（Slack, Mock等）を保持し、通知設定の種類に応じて適切なSenderに委譲します。
pub struct NotificationRouter {
    config: ResourceConfig,
    slack_sender: SlackSender,
    mock_sender: MockSender,
}

impl NotificationRouter {
    pub fn new(config: ResourceConfig) -> Self {
        Self {
            config,
            slack_sender: SlackSender::new(),
            mock_sender: MockSender::new(),
        }
    }

    fn format_message(&self, event: &NotificationEvent) -> String {
        let usage = match event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        };

        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period());
        let user_name = usage.user().name();

        match event {
            NotificationEvent::ResourceUsageCreated(_) => {
                let notes = usage
                    .notes()
                    .map(|n| format!(" ({})", n))
                    .unwrap_or_default();

                format!(
                    "✨ [新規使用予定] {}\n⏰ 期間: {}\n🖥️ 資源:\n{}{}",
                    user_name, time_period, resources, notes
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "♻️ [使用予定更新] {}\n⏰ 期間: {}\n🖥️ 資源:\n{}",
                    user_name, time_period, resources
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ [使用予定削除] {}\n⏰ 期間: {}\n🖥️ 資源:\n{}",
                    user_name, time_period, resources
                )
            }
        }
    }

    fn collect_notification_configs(&self, event: &NotificationEvent) -> Vec<NotificationConfig> {
        let resources = match event {
            NotificationEvent::ResourceUsageCreated(usage) => usage.resources(),
            NotificationEvent::ResourceUsageUpdated(usage) => usage.resources(),
            NotificationEvent::ResourceUsageDeleted(usage) => usage.resources(),
        };

        let mut configs = HashSet::new();
        for resource in resources {
            let resource_configs = self.config.get_notifications_for_resource(resource);
            configs.extend(resource_configs);
        }

        configs.into_iter().collect()
    }

    async fn send_to_destination(
        &self,
        config: &NotificationConfig,
        message: &str,
    ) -> Result<(), NotificationError> {
        match config {
            NotificationConfig::Slack { webhook_url } => {
                self.slack_sender.send(webhook_url.as_str(), message).await
            }
            NotificationConfig::Mock {} => self.mock_sender.send(&(), message).await,
        }
    }
}

#[async_trait]
impl Notifier for NotificationRouter {
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError> {
        let message = self.format_message(&event);
        let notification_configs = self.collect_notification_configs(&event);

        if notification_configs.is_empty() {
            // 通知先が設定されていない場合は何もしない
            return Ok(());
        }

        let mut errors = Vec::new();

        // 各通知設定に対して送信（ベストエフォート）
        for config in &notification_configs {
            if let Err(e) = self.send_to_destination(config, &message).await {
                eprintln!("⚠️  通知送信エラー: {}", e); // TODO: エラーハンドリングの改善
                errors.push(e);
            }
        }

        Ok(())
    }
}
