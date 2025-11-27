//! Slackイベントゲートウェイ
//!
//! 受信したSlackイベントを適切なハンドラにルーティング

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use slack_morphism::prelude::*;
use tracing::{error, info};

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackApp<R> {
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
            "/reserve" => {
                crate::interface::slack::slash_commands::reserve::handle(self, event).await
            }
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
    /// * `event` - Slackからのインタラクションイベント（ボタンクリック、モーダル送信など）
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
            SlackInteractionEvent::BlockActions(block_actions) => {
                self.route_block_actions(block_actions).await?;
                Ok(None)
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
            Some(CALLBACK_LINK_USER) => {
                info!("  → ユーザーリンクモーダル");
                crate::interface::slack::view_submissions::link_user::handle(self, view_submission)
                    .await
            }
            Some(CALLBACK_RESERVE_SUBMIT) => {
                info!("  → 予約モーダル");
                crate::interface::slack::view_submissions::reservation::handle(
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

    /// ブロックアクションイベントをルーティング（ボタンクリック、セレクトメニューなど）
    async fn route_block_actions(
        &self,
        block_actions: &SlackInteractionBlockActionsEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("📋 ブロックアクションを処理中");

        // モーダル内のインタラクションを処理（viewがSome）
        if block_actions.view.is_some() {
            return self.route_modal_interactions(block_actions).await;
        }

        Ok(())
    }

    /// モーダル内のインタラクションをルーティング（リソースタイプ変更、サーバー選択など）
    async fn route_modal_interactions(
        &self,
        block_actions: &SlackInteractionBlockActionsEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("  → モーダル内のインタラクション");

        let Some(actions) = &block_actions.actions else {
            return Ok(());
        };

        for action in actions {
            let action_id = action.action_id.to_string();

            match action_id.as_str() {
                ACTION_RESERVE_RESOURCE_TYPE | ACTION_RESERVE_SERVER_SELECT => {
                    crate::interface::slack::block_actions::modal_state_change::handle(
                        self,
                        block_actions,
                        action,
                    )
                    .await?
                }
                _ => {
                    // その他のモーダルアクションは送信時に処理
                    info!("  → アクション {} （送信時に処理）", action_id);
                }
            }
        }

        Ok(())
    }
}
