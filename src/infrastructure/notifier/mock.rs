use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::ports::notifier::{NotificationError, NotificationEvent, Notifier};
use async_trait::async_trait;

pub struct MockNotifier;

impl MockNotifier {
    pub fn new() -> Self {
        Self
    }

    fn format_message(&self, event: &NotificationEvent) -> String {
        match event {
            NotificationEvent::ResourceUsageCreated(usage) => {
                let resources = usage
                    .resources()
                    .iter()
                    .map(|item| match item {
                        Resource::Gpu(spec) => {
                            format!(
                                "サーバー: {}, モデル: {}, デバイスID: {}",
                                spec.server(),
                                spec.model(),
                                spec.device_number()
                            )
                        }
                        Resource::Room { name } => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let notes = usage
                    .notes()
                    .map(|n| format!(" ({})", n))
                    .unwrap_or_default();

                format!(
                    "[新規使用予定] {}\n期間: {} - {}\n資源: {}{}",
                    usage.user().name(),
                    usage.time_period().start().format("%Y-%m-%d %H:%M"),
                    usage.time_period().end().format("%Y-%m-%d %H:%M"),
                    resources,
                    notes
                )
            }
            NotificationEvent::ResourceUsageUpdated(usage) => {
                let resources = usage
                    .resources()
                    .iter()
                    .map(|item| match item {
                        Resource::Gpu(spec) => {
                            format!(
                                "サーバー: {}, モデル: {}, デバイスID: {}",
                                spec.server(),
                                spec.model(),
                                spec.device_number()
                            )
                        }
                        Resource::Room { name } => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(
                    "[使用予定更新] {}\n期間: {} - {}\n資源: {}",
                    usage.user().name(),
                    usage.time_period().start().format("%Y-%m-%d %H:%M"),
                    usage.time_period().end().format("%Y-%m-%d %H:%M"),
                    resources
                )
            }
            NotificationEvent::ResourceUsageDeleted { user, .. } => {
                format!("[使用予定削除] {}が使用予定を削除しました", user.name())
            }
        }
    }
}

#[async_trait]
impl Notifier for MockNotifier {
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError> {
        let message = self.format_message(&event);
        println!("{}", message);
        Ok(())
    }
}
