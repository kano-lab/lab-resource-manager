//! Slackイベントゲートウェイ
//!
//! 受信したSlackイベントを適切なハンドラにルーティング

use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use slack_morphism::prelude::*;
use tracing::{error, info};

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

    /// インタラクションイベントをルーティング
    ///
    /// # 引数
    /// * `event` - Slackからのインタラクションイベント（モーダル送信など）
    ///
    /// # 戻り値
    /// View Submissionの場合はレスポンス（結果モーダルなど）を返す
    pub async fn route_interaction(
        &self,
        event: SlackInteractionEvent,
    ) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
        info!("🔘 インタラクションイベントを受信");

        match &event {
            SlackInteractionEvent::ViewSubmission(view_submission) => {
                self.route_view_submission(view_submission).await
            }
            SlackInteractionEvent::ViewClosed(_) => {
                info!("  → ViewClosedイベント（無視）");
                Ok(None)
            }
            _ => {
                info!("  → 不明なインタラクションイベント（無視）");
                Ok(None)
            }
        }
    }

    /// ビュー送信イベントをルーティング（モーダル送信）
    async fn route_view_submission(
        &self,
        view_submission: &SlackInteractionViewSubmissionEvent,
    ) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
        info!("📝 ビュー送信を処理中");

        // callback_idを抽出してどのモーダルが送信されたかを判定
        let callback_id = match &view_submission.view.view {
            SlackView::Modal(modal) => modal.callback_id.as_ref().map(|id| id.to_string()),
            _ => None,
        };

        match callback_id.as_deref() {
            Some(CALLBACK_REGISTER_EMAIL) => {
                info!("  → メールアドレス登録モーダル");
                crate::interface::slack::view_submissions::registration::handle(
                    self,
                    view_submission,
                )
                .await
            }
            _ => {
                error!("❌ 不明なcallback_id: {:?}", callback_id);
                Ok(None)
            }
        }
    }
}
