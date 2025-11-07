use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use async_trait::async_trait;

use super::sender::{NotificationContext, Sender};

/// 標準出力にメッセージを送信するテスト用実装
pub struct MockSender;

impl Default for MockSender {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSender {
    pub fn new() -> Self {
        Self
    }

    /// イベントから簡易的なメッセージを構築
    fn format_message(&self, context: &NotificationContext) -> String {
        let usage = match context.event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        };

        let user = usage.owner_email().as_str();
        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period(), context.timezone);

        match context.event {
            NotificationEvent::ResourceUsageCreated(_) => {
                format!(
                    "🔔 新規予約\n{} が {} を予約しました\n期間: {}",
                    user, resources, time_period
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "🔄 予約更新\n{} が {} の予約を変更しました\n期間: {}",
                    user, resources, time_period
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ 予約削除\n{} が {} の予約をキャンセルしました\n期間: {}",
                    user, resources, time_period
                )
            }
        }
    }
}

#[async_trait]
impl Sender for MockSender {
    type Config = ();

    async fn send(
        &self,
        _config: &(),
        context: NotificationContext<'_>,
    ) -> Result<(), NotificationError> {
        let message = self.format_message(&context);
        println!("📤 [MockSender]");
        println!("{}", message);
        println!();
        Ok(())
    }
}
