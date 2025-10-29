use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;

use super::Sender;

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
}

#[async_trait]
impl Sender for MockSender {
    type Config = ();

    async fn send(&self, _config: &(), message: &str) -> Result<(), NotificationError> {
        println!("📤 [MockSender]");
        println!("{}", message);
        println!();
        Ok(())
    }
}
