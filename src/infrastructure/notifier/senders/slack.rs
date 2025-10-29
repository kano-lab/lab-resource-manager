use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use super::Sender;

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
}

#[async_trait]
impl Sender for SlackSender {
    type Config = str;

    async fn send(&self, webhook_url: &str, message: &str) -> Result<(), NotificationError> {
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
