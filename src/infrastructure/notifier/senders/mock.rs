use crate::domain::ports::notifier::{NotificationError, NotificationEvent};
use crate::infrastructure::notifier::template_renderer::TemplateRenderer;
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
    /// 新しいMockSenderを作成
    pub fn new() -> Self {
        Self
    }

    /// イベントからテンプレートレンダラーを用いてメッセージを構築
    /// （Slack送信時と同等のフォーマット出力）
    fn format_message(&self, context: &NotificationContext) -> String {
        let renderer = TemplateRenderer::new(
            &context.customization.templates,
            &context.customization.format,
            context.timezone,
        );

        match context.event {
            NotificationEvent::ResourceUsageCreated(usage) => {
                renderer.render_created(usage, usage.owner_email().as_str())
            }
            NotificationEvent::ResourceUsageUpdated(usage) => {
                renderer.render_updated(usage, usage.owner_email().as_str())
            }
            NotificationEvent::ResourceUsageDeleted(usage) => {
                renderer.render_deleted(usage, usage.owner_email().as_str())
            }
            NotificationEvent::UnauthorizedUsageDetected {
                reserved_usage,
                actual_user_email,
            } => {
                let actual_display = actual_user_email
                    .as_ref()
                    .map(|e| e.as_str().to_string())
                    .unwrap_or_else(|| "不明".to_string());
                renderer.render_unauthorized(
                    reserved_usage,
                    reserved_usage.owner_email().as_str(),
                    &actual_display,
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
