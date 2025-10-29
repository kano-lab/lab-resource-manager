use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use super::sender::{NotificationContext, Sender};

/// Slack Webhook経由でメッセージを送信する
pub struct SlackSender {
    client: Client,
}

impl SlackSender {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// メッセージ内のメールアドレスをSlackメンションに置き換える
    fn format_message_with_mentions(&self, context: NotificationContext) -> String {
        let message = context.message;

        // IdentityLinkがあり、Slack IDが取得できる場合のみメンション化
        if let Some(identity) = context.identity_link {
            if let Some(slack_identity) = identity.get_identity_for_system(&ExternalSystem::Slack) {
                // メールアドレスをSlackメンション形式に置き換え
                let email = context.user_email.as_str();
                return message.replace(email, &format!("<@{}>", slack_identity.user_id()));
            }
        }

        // IdentityLinkがない、またはSlack IDがない場合はそのまま
        message.to_string()
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
        let formatted_message = self.format_message_with_mentions(context);

        let payload = json!({
            "text": formatted_message
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
