use super::commands::SlackCommandHandler;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slack Socket Mode Bot
///
/// This is a simplified wrapper that will be used with Socket Mode.
/// The actual Socket Mode server setup should be done in the binary
/// (main.rs or bin file) using slack-morphism's Socket Mode facilities.
pub struct SlackBot {
    command_handler: Arc<SlackCommandHandler>,
    client: Arc<SlackHyperClient>,
}

impl SlackBot {
    /// 新しいボットインスタンスを作成
    ///
    /// # 引数
    /// * `bot_token` - Bot User OAuth Token (xoxb-...)
    /// * `command_handler` - コマンドハンドラ
    pub async fn new(
        command_handler: Arc<SlackCommandHandler>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        Ok(Self {
            command_handler,
            client,
        })
    }

    /// Slashコマンドを処理
    pub async fn handle_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.command_handler.route_slash_command(event).await
    }

    /// クライアントへの参照を取得
    pub fn client(&self) -> Arc<SlackHyperClient> {
        self.client.clone()
    }
}
