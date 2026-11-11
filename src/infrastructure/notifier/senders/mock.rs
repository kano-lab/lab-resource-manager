use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::notifier::template_renderer::{TemplateRenderer, render_event};
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

        let user_display = context.event.owner_email().as_str();
        render_event(&renderer, context.event, user_display)
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
