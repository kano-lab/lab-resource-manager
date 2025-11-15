//! Slackイベントゲートウェイ
//!
//! 受信したSlackイベントを適切なハンドラにルーティング

use crate::interface::slack::app::SlackApp;
use slack_morphism::prelude::*;
use tracing::info;

impl SlackApp {
    /// スラッシュコマンドイベントをルーティング
    ///
    /// # 引数
    /// * `event` - Slackからのスラッシュコマンドイベント
    ///
    /// # 戻り値
    /// Slackに返すレスポンス
    pub async fn route_slash_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let command = event.command.0.as_str();
        info!("📨 スラッシュコマンドを受信: {}", command);

        match command {
            "/register-calendar" => {
                crate::interface::slack::slash_commands::register_calendar::handle(self, event)
                    .await
            }
            "/link-user" => {
                crate::interface::slack::slash_commands::link_user::handle(self, event).await
            }
            _ => Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!("不明なコマンド: {}", command)),
            )),
        }
    }
}
