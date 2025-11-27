//! Slackイベントゲートウェイ
//!
//! 受信したSlackイベントを適切なハンドラにルーティング

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use slack_morphism::prelude::*;
use tracing::error;

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

        // user_id -> channel_id マッピングを更新
        self.user_channel_map
            .write()
            .unwrap()
            .insert(event.user_id.clone(), event.channel_id.clone());

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

        match &event {
            SlackInteractionEvent::ViewSubmission(view_submission) => {
                self.route_view_submission(view_submission).await
            }
            SlackInteractionEvent::BlockActions(block_actions) => {
                self.route_block_actions(block_actions).await?;
                Ok(None)
            }
            SlackInteractionEvent::ViewClosed(_) => {
                Ok(None)
            }
            _ => {
                Ok(None)
            }
        }
    }

    /// ビュー送信イベントをルーティング（モーダル送信）
    async fn route_view_submission(
        &self,
        view_submission: &SlackInteractionViewSubmissionEvent,
    ) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {

        // callback_idを抽出してどのモーダルが送信されたかを判定
        let callback_id = match &view_submission.view.view {
            SlackView::Modal(modal) => modal.callback_id.as_ref().map(|id| id.to_string()),
            _ => None,
        };


        match callback_id.as_deref() {
            Some(CALLBACK_REGISTER_EMAIL) => {
                crate::interface::slack::view_submissions::registration::handle(
                    self,
                    view_submission,
                )
                .await
            }
            Some(CALLBACK_LINK_USER) => {
                crate::interface::slack::view_submissions::link_user::handle(self, view_submission)
                    .await
            }
            Some(CALLBACK_RESERVE_SUBMIT) => {
                crate::interface::slack::view_submissions::reserve::handle(self, view_submission)
                    .await
            }
            Some(CALLBACK_RESERVE_UPDATE) => {
                crate::interface::slack::view_submissions::update::handle(self, view_submission)
                    .await
            }
            _ => {
                error!("❌ 不明なcallback_id: {:?}", callback_id);
                Ok(None)
            }
        }
    }

    /// ブロックアクションイベントをルーティング（ボタンクリック、セレクトメニューなど）
    ///
    /// # 引数
    /// * `block_actions` - Slackからのブロックアクションイベント（ボタンクリック、セレクトメニューなど）
    ///
    /// # 戻り値
    /// 正常に処理された場合は `Ok(())` を返す
    ///
    /// # エラー
    /// 処理中にエラーが発生した場合は `Err` を返す
    async fn route_block_actions(
        &self,
        block_actions: &SlackInteractionBlockActionsEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

        // モーダル内のインタラクションを処理（viewがSome）
        if block_actions.view.is_some() {
            return self.route_modal_interactions(block_actions).await;
        }

        // メッセージ内のボタンを処理
        let Some(actions) = &block_actions.actions else {
            return Ok(());
        };

        for action in actions {
            let action_id = action.action_id.to_string();

            match action_id.as_str() {
                ACTION_EDIT_RESERVATION => {
                    crate::interface::slack::block_actions::edit_button::handle(
                        self,
                        block_actions,
                        action,
                    )
                    .await?
                }
                ACTION_CANCEL_RESERVATION => {
                    crate::interface::slack::block_actions::cancel_button::handle(
                        self,
                        block_actions,
                        action,
                    )
                    .await?
                }
                _ => {
                }
            }
        }

        Ok(())
    }

    /// モーダル内のインタラクションをルーティング（リソースタイプ変更、サーバー選択など）
    async fn route_modal_interactions(
        &self,
        block_actions: &SlackInteractionBlockActionsEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

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
                }
            }
        }

        Ok(())
    }
}
