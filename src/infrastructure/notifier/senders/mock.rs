use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;

use super::sender::{NotificationContext, Sender};

/// 標準出力にメッセージを送信するテスト用実装
pub struct MockSender;

impl MockSender {
    pub fn new() -> Self {
        Self
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
        println!("📤 [MockSender]");
        println!("{}", context.message);
        println!();
        Ok(())
    }
}
