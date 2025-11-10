use crate::application::usecases::{
    create_resource_usage::CreateResourceUsageUseCase,
    delete_resource_usage::DeleteResourceUsageUseCase,
    grant_user_resource_access::GrantUserResourceAccessUseCase,
    update_resource_usage::UpdateResourceUsageUseCase,
};
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod, UsageId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::constants::*;
use crate::interface::slack::parsers::{parse_datetime, parse_device_id};
use crate::interface::slack::views::{create_register_email_modal, create_reserve_modal};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::task::TaskTracker;
use tracing::{error, info};

/// Slackコマンドハンドラ
pub struct SlackCommandHandler<R: ResourceUsageRepository> {
    grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    create_usage_usecase: Option<Arc<CreateResourceUsageUseCase<R>>>,
    delete_usage_usecase: Option<Arc<DeleteResourceUsageUseCase<R>>>,
    update_usage_usecase: Option<Arc<UpdateResourceUsageUseCase<R>>>,
    identity_repo: Option<Arc<dyn crate::domain::ports::repositories::IdentityLinkRepository>>,
    resource_config: Option<Arc<ResourceConfig>>,
    slack_client: Option<Arc<SlackHyperClient>>,
    bot_token: Option<SlackApiToken>,
    task_tracker: TaskTracker,
    http_client: reqwest::Client,
}

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackCommandHandler<R> {
    /// 新しいSlackコマンドハンドラを作成
    ///
    /// # Arguments
    /// * `grant_access_usecase` - アクセス権付与ユースケース
    pub fn new(grant_access_usecase: Arc<GrantUserResourceAccessUseCase>) -> Self {
        Self {
            grant_access_usecase,
            create_usage_usecase: None,
            delete_usage_usecase: None,
            update_usage_usecase: None,
            identity_repo: None,
            resource_config: None,
            slack_client: None,
            bot_token: None,
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// ResourceUsage機能を追加（ビルダーパターン）
    pub fn with_resource_usage(
        mut self,
        repository: Arc<R>,
        identity_repo: Arc<dyn crate::domain::ports::repositories::IdentityLinkRepository>,
    ) -> Self {
        self.create_usage_usecase = Some(Arc::new(CreateResourceUsageUseCase::new(repository.clone())));
        self.delete_usage_usecase = Some(Arc::new(DeleteResourceUsageUseCase::new(repository.clone())));
        self.update_usage_usecase = Some(Arc::new(UpdateResourceUsageUseCase::new(repository)));
        self.identity_repo = Some(identity_repo);
        self
    }

    /// リソース設定を追加（ビルダーパターン）
    pub fn with_resource_config(mut self, config: Arc<ResourceConfig>) -> Self {
        self.resource_config = Some(config);
        self
    }

    /// SlackClientを追加（ビルダーパターン）
    pub fn with_slack_client(mut self, client: Arc<SlackHyperClient>) -> Self {
        self.slack_client = Some(client);
        self
    }

    /// Bot tokenを追加（ビルダーパターン）
    pub fn with_bot_token(mut self, token: SlackApiToken) -> Self {
        self.bot_token = Some(token);
        self
    }

    /// バックグラウンドタスクの完了を待機
    ///
    /// シャットダウン時に呼び出して、全てのバックグラウンドタスクの完了を待つ
    pub async fn shutdown(&self) {
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }

    /// Slashコマンドをルーティング
    pub async fn route_slash_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let command = event.command.0.as_str();
        let text = event.text.as_deref().unwrap_or("");
        let slack_user_id = event.user_id.to_string();
        let response_url = event.response_url.clone();
        let trigger_id = event.trigger_id.clone();

        match command {
            "/register-calendar" => {
                self.handle_register_calendar(text, slack_user_id, response_url)
                    .await
            }
            "/link-user" => self.handle_link_user(text, response_url).await,
            "/reserve" => {
                self.handle_reserve_command(trigger_id, slack_user_id)
                    .await
            }
            _ => Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!("不明なコマンド: {}", command)),
            )),
        }
    }

    async fn handle_register_calendar(
        &self,
        text: &str,
        slack_user_id: String,
        response_url: SlackResponseUrl,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        if text.is_empty() {
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text("⚠️  このコマンドは非推奨です。代わりに `/reserve` コマンドを使用してください。\n\n使い方: `/register-calendar <your-email@gmail.com>`".to_string()),
            ));
        }

        // 非推奨警告をログに記録
        info!("⚠️  非推奨コマンド /register-calendar が使用されました: user={}", slack_user_id);

        let grant_access_usecase = self.grant_access_usecase.clone();
        let email_str = text.to_string();

        self.execute_with_background_response(response_url, || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(ExternalSystem::Slack, slack_user_id, email.clone())
                .await
                .map_err(|e| format!("❌ カレンダー登録に失敗: {}", e))?;

            Ok(format!(
                "✅ 登録完了！カレンダーへのアクセス権を付与しました: {}\n\n⚠️  今後は `/reserve` コマンドを使用してください。このコマンドは非推奨です。",
                email.as_str()
            ))
        })
        .await
    }

    async fn handle_link_user(
        &self,
        text: &str,
        response_url: SlackResponseUrl,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() != 2 {
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text("使い方: `/link-user <@slack_user> <email@gmail.com>`".to_string()),
            ));
        }

        let grant_access_usecase = self.grant_access_usecase.clone();

        // Slackメンション形式のバリデーションとパース
        let slack_mention = parts[0].trim();
        let target_slack_user_id = slack_mention
            .strip_prefix("<@")
            .and_then(|s| s.strip_suffix(">"))
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string());

        let target_slack_user_id = match target_slack_user_id {
            Some(id) => id,
            None => {
                return Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text("❌ Slackユーザーの形式が不正です。".to_string()),
                ));
            }
        };

        let email_str = parts[1].to_string();

        self.execute_with_background_response(response_url, || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(
                    ExternalSystem::Slack,
                    target_slack_user_id.clone(),
                    email.clone(),
                )
                .await
                .map_err(|e| format!("❌ ユーザー紐付けに失敗: {}", e))?;

            Ok(format!(
                "✅ 紐付け完了！<@{}> に {} のカレンダーアクセス権を付与しました。",
                target_slack_user_id,
                email.as_str()
            ))
        })
        .await
    }

    /// バックグラウンドで処理を実行し、結果をSlackに送信する共通ヘルパー
    ///
    /// TaskTrackerを使用してタスクを追跡し、シャットダウン時のグレースフル終了を可能にする
    async fn execute_with_background_response<F, Fut>(
        &self,
        response_url: SlackResponseUrl,
        operation: F,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
    {
        let http_client = self.http_client.clone();
        self.task_tracker.spawn(async move {
            let message = match operation().await {
                Ok(msg) => msg,
                Err(err) => err,
            };

            Self::send_followup_message_static(&http_client, &response_url, message).await;
        });

        Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new().with_text("⏳ 処理中...".to_string()),
        ))
    }

    /// Slackにフォローアップメッセージを送信
    ///
    /// バックグラウンドタスクから呼び出すための静的メソッド
    async fn send_followup_message_static(
        http_client: &reqwest::Client,
        response_url: &SlackResponseUrl,
        message: String,
    ) {
        let payload = serde_json::json!({
            "text": message,
            "response_type": "in_channel"
        });

        match http_client
            .post(response_url.0.as_str())
            .json(&payload)
            .send()
            .await
        {
            Ok(_) => info!("✅ フォローアップメッセージを送信しました"),
            Err(e) => error!("フォローアップメッセージの送信に失敗: {}", e),
        }
    }

    /// /reserveコマンド - モーダルを開く
    async fn handle_reserve_command(
        &self,
        trigger_id: SlackTriggerId,
        user_id: String,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let config = match &self.resource_config {
            Some(cfg) => cfg,
            None => {
                return Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text("❌ リソース設定が読み込まれていません".to_string()),
                ))
            }
        };

        let client = match &self.slack_client {
            Some(c) => c,
            None => {
                return Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text("❌ Slackクライアントが初期化されていません".to_string()),
                ))
            }
        };

        let bot_token = match &self.bot_token {
            Some(t) => t,
            None => {
                return Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text("❌ Bot tokenが設定されていません".to_string()),
                ))
            }
        };

        // ユーザーのリンク状態をチェック
        if let Some(identity_repo) = &self.identity_repo {
            match identity_repo.find_by_external_user_id(&ExternalSystem::Slack, &user_id).await {
                Ok(None) => {
                    // 未リンク: メールアドレス登録モーダルを表示
                    info!("ユーザー {} は未リンク。メールアドレス登録モーダルを表示します", user_id);
                    let modal = create_register_email_modal();
                    let session = client.open_session(bot_token);
                    let open_view_req = SlackApiViewsOpenRequest::new(trigger_id, modal);

                    match session.views_open(&open_view_req).await {
                        Ok(_) => {
                            info!("✅ メールアドレス登録モーダルを開きました");
                            return Ok(SlackCommandEventResponse::new(
                                SlackMessageContent::new(),
                            ));
                        }
                        Err(e) => {
                            error!("❌ メールアドレス登録モーダルを開けませんでした: {}", e);
                            return Ok(SlackCommandEventResponse::new(
                                SlackMessageContent::new()
                                    .with_text(format!("❌ モーダルを開けませんでした: {}", e)),
                            ));
                        }
                    }
                }
                Ok(Some(_)) => {
                    // リンク済み: 予約モーダルを表示
                    info!("ユーザー {} はリンク済み。予約モーダルを表示します", user_id);
                }
                Err(e) => {
                    error!("ユーザーリンク状態の確認に失敗: {}", e);
                    // エラーが起きた場合は従来通り予約モーダルを表示
                }
            }
        }

        // モーダルを作成（初期状態: GPU、最初のサーバーを選択）
        let initial_server = config.servers.first().map(|s| s.name.as_str());
        let modal = create_reserve_modal(config, None, initial_server, None);

        // モーダルを開く
        let session = client.open_session(bot_token);
        let open_view_req = SlackApiViewsOpenRequest::new(trigger_id, modal);

        match session.views_open(&open_view_req).await {
            Ok(_) => {
                info!("✅ モーダルを開きました");
                // モーダルが開いた場合、何も返さない（即座に応答済み）
                Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new(),
                ))
            }
            Err(e) => {
                error!("❌ モーダルを開けませんでした: {}", e);
                Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text(format!("❌ モーダルを開けませんでした: {}", e)),
                ))
            }
        }
    }

    /// メールアドレス登録用のモーダルを作成
    ///
    /// ユーザーが未リンクの場合に表示される


    /// インタラクション処理（ボタンクリックなど）
    pub async fn handle_interaction(
        &self,
        event: SlackInteractionEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔘 インタラクションを受信しました");

        // イベントタイプをログ出力
        match &event {
            SlackInteractionEvent::BlockActions(_) => info!("  → BlockActionsイベント"),
            SlackInteractionEvent::ViewSubmission(_) => info!("  → ViewSubmissionイベント"),
            SlackInteractionEvent::ViewClosed(_) => info!("  → ViewClosedイベント"),
            _ => info!("  → その他のイベント"),
        }

        // ViewSubmissionイベントを処理
        if let SlackInteractionEvent::ViewSubmission(view_submission) = &event {
            info!("📝 モーダル送信を処理中...");

            // callback_idをチェック
            if let SlackView::Modal(modal) = &view_submission.view.view {
                if let Some(callback_id) = &modal.callback_id {
                    if callback_id.to_string() == CALLBACK_REGISTER_EMAIL {
                        info!("  → メールアドレス登録モーダルの送信を検出");

                        // モーダルから値を抽出してユーザー登録を行う
                        match self.process_registration_submission(view_submission).await {
                            Ok(_) => {
                                info!("✅ ユーザー登録を完了しました");
                                return Ok(());
                            }
                            Err(e) => {
                                error!("❌ ユーザー登録に失敗: {}", e);
                                return Err(e);
                            }
                        }
                    } else if callback_id.to_string() == CALLBACK_RESERVE_SUBMIT {
                        info!("  → 予約モーダルの送信を検出");

                        // モーダルから値を抽出して予約を作成
                        match self.process_reservation_submission(view_submission).await {
                            Ok(_) => {
                                info!("✅ 予約を作成しました");
                                return Ok(());
                            }
                            Err(e) => {
                                error!("❌ 予約作成に失敗: {}", e);
                                return Err(e);
                            }
                        }
                    } else if callback_id.to_string() == CALLBACK_UPDATE_SUBMIT {
                        info!("  → 予約更新モーダルの送信を検出");

                        // モーダルから値を抽出して予約を更新
                        match self.process_update_submission(view_submission).await {
                            Ok(_) => {
                                info!("✅ 予約を更新しました");
                                return Ok(());
                            }
                            Err(e) => {
                                error!("❌ 予約更新に失敗: {}", e);
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }

        // block_actionsイベントのみ処理
        if let SlackInteractionEvent::BlockActions(block_actions) = &event {
            info!("📋 BlockActionsイベントを処理中...");

            // メッセージ内のボタンクリック処理（viewがNoneの場合）
            if block_actions.view.is_none() {
                info!("  → メッセージ内のボタンクリック");
                if let Some(actions) = &block_actions.actions {
                    for action in actions {
                        let action_id = action.action_id.to_string();
                        info!("  → アクションID: {}", action_id);

                        if action_id == ACTION_EDIT_RESERVATION {
                            // 更新ボタンがクリックされた
                            if let Some(usage_id_str) = &action.value {
                                info!("🔄 予約更新要求: usage_id={}", usage_id_str);
                                if let Some(user) = &block_actions.user {
                                    let trigger_id = &block_actions.trigger_id;
                                    match self.handle_edit_reservation(&user.id, usage_id_str, trigger_id).await {
                                        Ok(_) => {
                                            info!("✅ 更新モーダルを開きました");
                                        }
                                        Err(e) => {
                                            error!("❌ 更新モーダルを開けませんでした: {}", e);
                                        }
                                    }
                                } else {
                                    error!("❌ ユーザー情報が取得できませんでした");
                                }
                            }
                        } else if action_id == ACTION_CANCEL_RESERVATION {
                            // キャンセルボタンがクリックされた
                            if let Some(usage_id_str) = &action.value {
                                info!("🗑️ 予約キャンセル要求: usage_id={}", usage_id_str);
                                if let Some(user) = &block_actions.user {
                                    match self.handle_cancel_reservation(&user.id, usage_id_str).await {
                                        Ok(_) => {
                                            info!("✅ 予約をキャンセルしました");
                                        }
                                        Err(e) => {
                                            error!("❌ 予約キャンセルに失敗: {}", e);
                                        }
                                    }
                                } else {
                                    error!("❌ ユーザー情報が取得できませんでした");
                                }
                            }
                        }
                    }
                }
            }

            // モーダル内のアクションのみ処理
            if let Some(SlackView::Modal(_modal_view)) = &block_actions.view {
                info!("  → モーダル内のアクション");
                // リソース設定を取得
                let config = match &self.resource_config {
                    Some(cfg) => cfg,
                    None => {
                        error!("リソース設定が読み込まれていません");
                        return Ok(());
                    }
                };

                // アクションを確認
                if let Some(actions) = &block_actions.actions {
                    info!("  → {} 個のアクションを検出", actions.len());
                    for action in actions {
                        let action_id = action.action_id.to_string();
                        info!("  → アクションID: {}", action_id);

                        // リソースタイプ変更またはサーバー選択の場合、モーダルを更新
                        if action_id == ACTION_RESERVE_RESOURCE_TYPE || action_id == ACTION_RESERVE_SERVER_SELECT {
                            info!("🔄 モーダル更新トリガー検出: {}", action_id);
                            // 現在のモーダルの状態から値を取得
                            let (resource_type, selected_server) = self.extract_modal_state_from_block_actions(block_actions);

                            // アクションから新しい選択値を取得
                            let new_resource_type = if action_id == ACTION_RESERVE_RESOURCE_TYPE {
                                // ラジオボタンの選択値を取得（textから）
                                action.selected_option.as_ref().and_then(|opt| {
                                    match &opt.text {
                                        SlackBlockText::Plain(plain) => {
                                            // "GPU Server" or "Room" from text
                                            let text_val = plain.text.as_str();
                                            if text_val == "GPU Server" {
                                                Some("gpu")
                                            } else if text_val == "Room" {
                                                Some("room")
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None
                                    }
                                })
                            } else {
                                resource_type
                            };

                            let new_selected_server = if action_id == ACTION_RESERVE_SERVER_SELECT {
                                // セレクトメニューの選択値を取得（textから）
                                action.selected_option.as_ref().and_then(|opt| {
                                    match &opt.text {
                                        SlackBlockText::Plain(plain) => Some(plain.text.as_str()),
                                        _ => None
                                    }
                                })
                            } else {
                                selected_server
                            };

                            // view_idをcontainerから取得
                            let view_id = match &block_actions.container {
                                SlackInteractionActionContainer::View(view_container) => {
                                    info!("  → view_id取得成功: {}", view_container.view_id.to_string());
                                    view_container.view_id.clone()
                                }
                                SlackInteractionActionContainer::Message(_) => {
                                    error!("❌ モーダル外のインタラクションです");
                                    continue;
                                }
                            };

                            info!("📝 選択値: type={:?}, server={:?}",
                                  new_resource_type, new_selected_server);

                            // 新しいモーダルを作成
                            info!("🔨 新しいモーダルを作成中...");
                            let updated_modal = create_reserve_modal(
                                config,
                                new_resource_type,
                                new_selected_server,
                                None, // モーダル更新時はusage_idなし
                            );

                            // モーダルを更新
                            info!("🚀 Slack APIにモーダル更新をリクエスト中...");
                            if let Err(e) = self.update_modal(view_id.clone(), updated_modal).await {
                                error!("❌ モーダルの更新に失敗: {}", e);
                            } else {
                                info!("✅ モーダルを動的に更新しました (view_id: {})", view_id.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// block_actionsイベントからモーダルの現在状態を抽出
    fn extract_modal_state_from_block_actions(&self, _block_actions: &SlackInteractionBlockActionsEvent) -> (Option<&str>, Option<&str>) {
        let resource_type: Option<&str> = None;
        let selected_server: Option<&str> = None;

        // view.stateから値を取得（view.stateが存在する場合）
        // 注：SlackModalView自体にstateフィールドがない場合は、
        // block_actionsのアクション自体から値を取得するか、
        // 別の方法でステートを追跡する必要があります

        // 実装をシンプルにするため、デフォルト値を使用
        // 実際のステート管理が必要な場合は、別途実装が必要

        (resource_type, selected_server)
    }

    /// モーダルを更新
    async fn update_modal(
        &self,
        view_id: SlackViewId,
        new_view: SlackView,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔧 update_modal開始 (view_id: {})", view_id.to_string());

        let client = match &self.slack_client {
            Some(c) => c,
            None => {
                error!("❌ Slackクライアントが初期化されていません");
                return Err("Slackクライアントが初期化されていません".into());
            }
        };

        let bot_token = match &self.bot_token {
            Some(t) => t,
            None => {
                error!("❌ Bot tokenが設定されていません");
                return Err("Bot tokenが設定されていません".into());
            }
        };

        info!("  → クライアントとトークン取得成功");

        let session = client.open_session(bot_token);
        let update_req = SlackApiViewsUpdateRequest::new(new_view)
            .with_view_id(view_id.clone());

        info!("  → Slack API views.update 呼び出し中...");
        match session.views_update(&update_req).await {
            Ok(response) => {
                info!("✅ views.update API成功: {:?}", response);
                Ok(())
            }
            Err(e) => {
                error!("❌ views.update API失敗: {:?}", e);
                Err(e.into())
            }
        }
    }

    /// ViewSubmissionイベントから予約を作成
    // TODO: Refactor this into interactions/modals::process_reservation_submission
    async fn process_reservation_submission(
        &self,
        view_submission: &SlackInteractionViewSubmissionEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔍 ViewSubmissionから値を抽出中...");

        // 必要な依存関係を確認
        let create_usage_usecase = match &self.create_usage_usecase {
            Some(uc) => uc.clone(),
            None => {
                return Err("CreateUsageUseCaseが設定されていません".into());
            }
        };

        let identity_repo = match &self.identity_repo {
            Some(repo) => repo.clone(),
            None => {
                return Err("IdentityRepositoryが設定されていません".into());
            }
        };

        let config = match &self.resource_config {
            Some(cfg) => cfg.clone(),
            None => {
                return Err("ResourceConfigが設定されていません".into());
            }
        };

        // stateから値を抽出
        let state = &view_submission.view.state_params.state;
        let values = match state {
            Some(s) => &s.values,
            None => {
                return Err("モーダルの状態が取得できませんでした".into());
            }
        };

        info!("  → 状態を取得しました。ブロック数: {}", values.len());

        // 各フィールドの値を抽出
        let mut resource_type: Option<String> = None;
        let mut server_name: Option<String> = None;
        let mut room_name: Option<String> = None;
        let mut device_ids: Vec<String> = Vec::new();
        let mut start_date: Option<String> = None;
        let mut start_time: Option<String> = None;
        let mut end_date: Option<String> = None;
        let mut end_time: Option<String> = None;
        let mut notes: Option<String> = None;

        // 全ブロックを走査して値を抽出
        for (_block_id, actions_map) in values.iter() {
            for (action_id, value) in actions_map.iter() {
                let action_id_str = action_id.to_string();
                info!("  → フィールド: {}", action_id_str);

                match action_id_str.as_str() {
                    ACTION_RESERVE_RESOURCE_TYPE => {
                        if let Some(selected) = &value.selected_option {
                            let text_val = &selected.text.text;
                            let type_val = if text_val == "GPU Server" {
                                "gpu"
                            } else if text_val == "Room" {
                                "room"
                            } else {
                                text_val.as_str()
                            };
                            resource_type = Some(type_val.to_string());
                            info!("    = リソースタイプ: {}", type_val);
                        }
                    }
                    ACTION_RESERVE_SERVER_SELECT => {
                        if let Some(selected) = &value.selected_option {
                            server_name = Some(selected.text.text.clone());
                            info!("    = サーバー: {}", selected.text.text);
                        }
                    }
                    ACTION_RESERVE_ROOM_SELECT => {
                        if let Some(selected) = &value.selected_option {
                            room_name = Some(selected.text.text.clone());
                            info!("    = 部屋: {}", selected.text.text);
                        }
                    }
                    ACTION_RESERVE_DEVICES => {
                        if let Some(selected_options) = &value.selected_options {
                            for opt in selected_options {
                                // "Device 0 (RTX 3090)" のようなフォーマットから数値を抽出
                                device_ids.push(opt.text.text.clone());
                            }
                            info!("    = デバイス: {:?}", device_ids);
                        }
                    }
                    ACTION_RESERVE_START_DATE => {
                        if let Some(date) = &value.selected_date {
                            start_date = Some(date.to_string());
                            info!("    = 開始日: {}", date);
                        }
                    }
                    ACTION_RESERVE_START_TIME => {
                        if let Some(time) = &value.selected_time {
                            start_time = Some(time.to_string());
                            info!("    = 開始時刻: {}", time);
                        }
                    }
                    ACTION_RESERVE_END_DATE => {
                        if let Some(date) = &value.selected_date {
                            end_date = Some(date.to_string());
                            info!("    = 終了日: {}", date);
                        }
                    }
                    ACTION_RESERVE_END_TIME => {
                        if let Some(time) = &value.selected_time {
                            end_time = Some(time.to_string());
                            info!("    = 終了時刻: {}", time);
                        }
                    }
                    ACTION_RESERVE_NOTES => {
                        if let Some(text) = &value.value {
                            notes = Some(text.clone());
                            info!("    = 備考: {}", text);
                        }
                    }
                    _ => {}
                }
            }
        }

        info!("📊 抽出完了");

        // 必須フィールドの検証
        let resource_type = resource_type.ok_or("リソースタイプが選択されていません")?;
        let start_date_str = start_date.ok_or("開始日が選択されていません")?;
        let start_time_str = start_time.ok_or("開始時刻が選択されていません")?;
        let end_date_str = end_date.ok_or("終了日が選択されていません")?;
        let end_time_str = end_time.ok_or("終了時刻が選択されていません")?;

        // SlackユーザーIDからメールアドレスを取得
        let slack_user_id = view_submission.user.id.to_string();
        let identity_link = identity_repo
            .find_by_external_user_id(&ExternalSystem::Slack, &slack_user_id)
            .await?
            .ok_or_else(|| {
                format!(
                    "Slackユーザー {} に対応するメールアドレスが見つかりません",
                    slack_user_id
                )
            })?;
        let owner_email = identity_link.email().clone();
        info!("  → ユーザー: {}", owner_email.as_str());

        // 日時をパースしてDateTime<Utc>に変換
        let start_datetime = parse_datetime(&start_date_str, &start_time_str)?;
        let end_datetime = parse_datetime(&end_date_str, &end_time_str)?;
        info!(
            "  → 期間: {} 〜 {}",
            start_datetime.format("%Y-%m-%d %H:%M"),
            end_datetime.format("%Y-%m-%d %H:%M")
        );

        // TimePeriodを作成
        let time_period = TimePeriod::new(start_datetime, end_datetime)
            .map_err(|e| format!("時間期間の作成に失敗: {}", e))?;

        // リソースを構築
        let resources = if resource_type == "gpu" {
            let server_name = server_name.ok_or("GPUサーバーが選択されていません")?;

            if device_ids.is_empty() {
                return Err("デバイスが選択されていません".into());
            }

            // サーバー設定を取得してデバイス情報を得る
            let server_config = config
                .get_server(&server_name)
                .ok_or_else(|| format!("サーバー設定が見つかりません: {}", server_name))?;

            // デバイスIDをパースしてGPUリソースを構築
            let mut gpu_resources = Vec::new();
            for device_text in &device_ids {
                // "Device 0 (RTX 3090)" のようなフォーマットから数値を抽出
                let device_id = parse_device_id(device_text)?;

                // デバイス設定から情報を取得
                let device_config = server_config
                    .devices
                    .iter()
                    .find(|d| d.id == device_id)
                    .ok_or_else(|| format!("デバイス {} が見つかりません", device_id))?;

                gpu_resources.push(Resource::Gpu(Gpu::new(
                    server_name.clone(),
                    device_id,
                    device_config.model.clone(),
                )));
            }
            gpu_resources
        } else if resource_type == "room" {
            let room_name = room_name.ok_or("部屋が選択されていません")?;
            vec![Resource::Room { name: room_name }]
        } else {
            return Err(format!("不明なリソースタイプ: {}", resource_type).into());
        };

        info!("  → リソース: {:?}", resources);

        // 予約を作成
        info!("📝 予約を作成中...");
        match create_usage_usecase
            .execute(owner_email, time_period, resources, notes)
            .await
        {
            Ok(usage_id) => {
                info!("✅ 予約を作成しました: {}", usage_id.as_str());
                Ok(())
            }
            Err(e) => {
                error!("❌ 予約作成に失敗: {}", e);
                Err(format!("予約作成に失敗: {}", e).into())
            }
        }
    }

    /// 予約更新処理（ViewSubmissionイベントから呼ばれる）
    // TODO: Refactor this into interactions/modals::process_update_submission
    async fn process_update_submission(
        &self,
        view_submission: &SlackInteractionViewSubmissionEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔍 ViewSubmissionから更新データを抽出中...");

        // 必要な依存関係を確認
        let update_usage_usecase = match &self.update_usage_usecase {
            Some(uc) => uc.clone(),
            None => {
                return Err("UpdateUsageUseCaseが設定されていません".into());
            }
        };

        let identity_repo = match &self.identity_repo {
            Some(repo) => repo.clone(),
            None => {
                return Err("IdentityRepositoryが設定されていません".into());
            }
        };

        // private_metadataからusage_idを取得
        let usage_id_str = if let SlackView::Modal(modal) = &view_submission.view.view {
            modal.private_metadata.as_ref()
                .ok_or("usage_idが見つかりません（private_metadataが空です）")?
                .as_str()
        } else {
            return Err("モーダルビューではありません".into());
        };

        let usage_id = UsageId::new(usage_id_str.to_string());
        info!("  → 更新対象の予約ID: {}", usage_id_str);

        // SlackユーザーIDからメールアドレスを取得（リンクチェック＋認可チェック）
        let slack_user_id = view_submission.user.id.to_string();
        let identity_link = identity_repo
            .find_by_external_user_id(&ExternalSystem::Slack, &slack_user_id)
            .await?
            .ok_or_else(|| {
                format!(
                    "Slackユーザー {} に対応するメールアドレスが見つかりません",
                    slack_user_id
                )
            })?;
        let owner_email = identity_link.email().clone();
        info!("  → ユーザー: {}", owner_email.as_str());

        // stateから値を抽出（process_reservation_submissionと同じロジック）
        let state = &view_submission.view.state_params.state;
        let values = match state {
            Some(s) => &s.values,
            None => {
                return Err("モーダルの状態が取得できませんでした".into());
            }
        };

        info!("  → 状態を取得しました。ブロック数: {}", values.len());

        // 各フィールドの値を抽出
        let mut start_date: Option<String> = None;
        let mut start_time: Option<String> = None;
        let mut end_date: Option<String> = None;
        let mut end_time: Option<String> = None;
        let mut notes: Option<String> = None;

        // 全ブロックを走査して値を抽出
        for (_block_id, actions_map) in values.iter() {
            for (action_id, value) in actions_map.iter() {
                let action_id_str = action_id.to_string();

                match action_id_str.as_str() {
                    ACTION_RESERVE_START_DATE => {
                        if let Some(date) = &value.selected_date {
                            start_date = Some(date.to_string());
                        }
                    }
                    ACTION_RESERVE_START_TIME => {
                        if let Some(time) = &value.selected_time {
                            start_time = Some(time.to_string());
                        }
                    }
                    ACTION_RESERVE_END_DATE => {
                        if let Some(date) = &value.selected_date {
                            end_date = Some(date.to_string());
                        }
                    }
                    ACTION_RESERVE_END_TIME => {
                        if let Some(time) = &value.selected_time {
                            end_time = Some(time.to_string());
                        }
                    }
                    ACTION_RESERVE_NOTES => {
                        if let Some(text) = &value.value {
                            notes = Some(text.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        info!("📊 抽出完了");

        // 必須フィールドの検証
        let start_date_str = start_date.ok_or("開始日が選択されていません")?;
        let start_time_str = start_time.ok_or("開始時刻が選択されていません")?;
        let end_date_str = end_date.ok_or("終了日が選択されていません")?;
        let end_time_str = end_time.ok_or("終了時刻が選択されていません")?;

        // 日時をパースしてDateTime<Utc>に変換
        let start_datetime = parse_datetime(&start_date_str, &start_time_str)?;
        let end_datetime = parse_datetime(&end_date_str, &end_time_str)?;
        info!(
            "  → 期間: {} 〜 {}",
            start_datetime.format("%Y-%m-%d %H:%M"),
            end_datetime.format("%Y-%m-%d %H:%M")
        );

        // TimePeriodを作成
        let time_period = TimePeriod::new(start_datetime, end_datetime)
            .map_err(|e| format!("時間期間の作成に失敗: {}", e))?;

        // 予約を更新
        info!("📝 予約を更新中...");
        match update_usage_usecase
            .execute(&usage_id, &owner_email, Some(time_period), notes)
            .await
        {
            Ok(_) => {
                info!("✅ 予約を更新しました: {}", usage_id_str);
                Ok(())
            }
            Err(e) => {
                error!("❌ 予約更新に失敗: {}", e);
                Err(format!("予約更新に失敗: {}", e).into())
            }
        }
    }

    /// モーダル送信処理
    pub async fn handle_view_submission(
        &self,
        view: SlackView,
        user_id: SlackUserId,
    ) -> Result<SlackViewSubmissionResponse, Box<dyn std::error::Error + Send + Sync>> {
        info!("モーダル送信を受信しました: user={}", user_id);

        // callback_idをチェック
        let callback_id = match &view {
            SlackView::Modal(modal) => modal.callback_id.as_ref().map(|id| id.0.as_str()),
            _ => None,
        };

        match callback_id {
            Some(CALLBACK_RESERVE_SUBMIT) => {
                self.handle_reserve_submission(view, user_id).await
            }
            _ => {
                error!("不明なcallback_id: {:?}", callback_id);
                Ok(SlackViewSubmissionResponse::Clear(
                    SlackViewSubmissionClearResponse::new(),
                ))
            }
        }
    }

    /// メールアドレス登録処理（ViewSubmissionイベントから呼ばれる）
    // TODO: Refactor this into interactions/modals::process_registration_submission
    async fn process_registration_submission(
        &self,
        view_submission: &SlackInteractionViewSubmissionEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("メールアドレス登録を処理中...");

        // ユーザーIDを取得
        let user_id = view_submission.user.id.to_string();

        // state から値を取得
        let state = &view_submission.view.state_params.state;
        let state_values = match state {
            Some(s) => &s.values,
            None => {
                return Err("モーダルの状態が取得できませんでした".into());
            }
        };

        // メールアドレスを取得
        let email_value = state_values
            .get(&SlackBlockId::new(ACTION_EMAIL_INPUT.to_string()))
            .and_then(|actions| actions.get(&SlackActionId::new(ACTION_EMAIL_INPUT.to_string())))
            .and_then(|value| {
                if let Some(plain_text_value) = &value.value {
                    Some(plain_text_value.clone())
                } else {
                    None
                }
            })
            .ok_or("メールアドレスが入力されていません")?;

        // バリデーション
        let email = EmailAddress::new(email_value.trim().to_string())
            .map_err(|e| format!("メールアドレスの形式が不正です: {}", e))?;

        // ユーザーを登録
        self.grant_access_usecase
            .execute(ExternalSystem::Slack, user_id.clone(), email.clone())
            .await
            .map_err(|e| format!("登録に失敗しました: {}", e))?;

        info!("✅ ユーザー登録成功: {}", email.as_str());

        // 登録成功後、自動的に予約モーダルを開く
        if let (Some(config), Some(client), Some(token), Some(trigger_id)) = (
            &self.resource_config,
            &self.slack_client,
            &self.bot_token,
            &view_submission.trigger_id,
        ) {
            info!("📋 予約モーダルを開きます...");

            // 予約モーダルを作成
            let initial_server = config.servers.first().map(|s| s.name.as_str());
            let reserve_modal = create_reserve_modal(config, None, initial_server, None);

            // views.open API を使用して新しいモーダルを開く
            let session = client.open_session(token);
            let open_request = SlackApiViewsOpenRequest::new(trigger_id.clone(), reserve_modal);

            match session.views_open(&open_request).await {
                Ok(_) => {
                    info!("✅ 予約モーダルを開きました");
                }
                Err(e) => {
                    error!("❌ 予約モーダルを開けませんでした: {}", e);
                    // エラーが起きても登録は成功しているので、エラーは返さない
                }
            }
        } else {
            info!("⚠️ 予約モーダルを開くための設定が不足しています（trigger_idが無い可能性があります）");
        }

        Ok(())
    }

    /// 予約作成モーダル送信処理
    async fn handle_reserve_submission(
        &self,
        view: SlackView,
        user_id: SlackUserId,
    ) -> Result<SlackViewSubmissionResponse, Box<dyn std::error::Error + Send + Sync>> {
        // 必要な依存関係を確認
        let _create_usage_usecase = match &self.create_usage_usecase {
            Some(uc) => uc.clone(),
            None => {
                error!("CreateUsageUseCaseが設定されていません");
                let mut errors = HashMap::new();
                errors.insert("error".to_string(), "システムエラー: 予約機能が利用できません".to_string());
                return Ok(SlackViewSubmissionResponse::Errors(
                    SlackViewSubmissionErrorsResponse::new(errors)
                ));
            }
        };

        let _identity_repo = match &self.identity_repo {
            Some(repo) => repo.clone(),
            None => {
                error!("IdentityRepositoryが設定されていません");
                let mut errors = HashMap::new();
                errors.insert("error".to_string(), "システムエラー: ID紐付け機能が利用できません".to_string());
                return Ok(SlackViewSubmissionResponse::Errors(
                    SlackViewSubmissionErrorsResponse::new(errors)
                ));
            }
        };

        let _config = match &self.resource_config {
            Some(cfg) => cfg.clone(),
            None => {
                error!("ResourceConfigが設定されていません");
                let mut errors = HashMap::new();
                errors.insert("error".to_string(), "システムエラー: リソース設定が読み込まれていません".to_string());
                return Ok(SlackViewSubmissionResponse::Errors(
                    SlackViewSubmissionErrorsResponse::new(errors)
                ));
            }
        };

        // TODO: モーダルの値を取得
        // SlackViewにはstate情報が含まれていないため、
        // SlackInteractionEventから直接取得する必要がある
        // 現在の設計では、handle_view_submissionの引数を変更する必要がある

        info!("モーダル送信を受信: view={:?}, user={}", view, user_id);

        // 仮実装: とりあえず成功として処理をクリア
        // 次のステップで正しいイベント処理を実装する
        info!("予約作成を受け付けました（実装中）");
        Ok(SlackViewSubmissionResponse::Clear(
            SlackViewSubmissionClearResponse::new()
        ))
    }
}

/// 日付文字列と時刻文字列をDateTime<Utc>に変換

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackCommandHandler<R> {
    /// キャンセルボタンのインタラクション処理
    // TODO: Refactor this into interactions/buttons::handle_cancel_reservation
    async fn handle_cancel_reservation(
        &self,
        slack_user_id: &SlackUserId,
        usage_id_str: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 必要な依存関係を確認
        let delete_usecase = match &self.delete_usage_usecase {
            Some(uc) => uc.clone(),
            None => {
                return Err("DeleteUsageUseCaseが設定されていません".into());
            }
        };

        let identity_repo = match &self.identity_repo {
            Some(repo) => repo.clone(),
            None => {
                return Err("IdentityRepositoryが設定されていません".into());
            }
        };

        // SlackユーザーIDからメールアドレスを取得
        let identity_link = identity_repo
            .find_by_external_user_id(&ExternalSystem::Slack, &slack_user_id.to_string())
            .await?
            .ok_or_else(|| {
                format!(
                    "Slackユーザー {} に対応するメールアドレスが見つかりません",
                    slack_user_id
                )
            })?;
        let owner_email = identity_link.email().clone();

        // 予約を削除
        let usage_id = UsageId::new(usage_id_str.to_string());
        delete_usecase.execute(&usage_id, &owner_email).await?;

        info!("✅ 予約 {} をキャンセルしました", usage_id_str);
        Ok(())
    }

    /// 予約更新ボタン処理
    // TODO: Refactor this into interactions/buttons::handle_edit_reservation
    async fn handle_edit_reservation(
        &self,
        slack_user_id: &SlackUserId,
        usage_id_str: &str,
        trigger_id: &SlackTriggerId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 必要な依存関係を確認
        let identity_repo = match &self.identity_repo {
            Some(repo) => repo.clone(),
            None => {
                return Err("IdentityRepositoryが設定されていません".into());
            }
        };

        let config = match &self.resource_config {
            Some(cfg) => cfg,
            None => {
                return Err("ResourceConfigが設定されていません".into());
            }
        };

        let client = match &self.slack_client {
            Some(c) => c,
            None => {
                return Err("Slackクライアントが初期化されていません".into());
            }
        };

        let bot_token = match &self.bot_token {
            Some(t) => t,
            None => {
                return Err("Bot tokenが設定されていません".into());
            }
        };

        // 未リンクチェック：SlackユーザーIDからメールアドレスを取得
        let identity_link = identity_repo
            .find_by_external_user_id(&ExternalSystem::Slack, &slack_user_id.to_string())
            .await?;

        if identity_link.is_none() {
            // 未リンクの場合はメールアドレス登録モーダルを表示
            info!("ユーザー {} は未リンク。メールアドレス登録モーダルを表示します", slack_user_id);
            let modal = create_register_email_modal();
            let session = client.open_session(bot_token);
            let open_view_req = SlackApiViewsOpenRequest::new(trigger_id.clone(), modal);

            session.views_open(&open_view_req).await?;
            return Ok(());
        }

        let _owner_email = identity_link.unwrap().email().clone();
        let _usage_id = UsageId::new(usage_id_str.to_string());

        // UpdateUseCaseが設定されているか確認
        if self.update_usage_usecase.is_none() {
            return Err("UpdateUsageUseCaseが設定されていません".into());
        }

        // TODO: 既存の予約データを取得してモーダルに反映
        // リポジトリに直接アクセスする方法がないため、
        // 一旦簡易的な実装として、デフォルト値でモーダルを開く
        info!("⚠️ 予約データの取得機能は未実装です。デフォルト値でモーダルを開きます。");

        // TODO: 既存の予約データを取得してモーダルに反映
        // 現状は新規予約と同じモーダルを開く（デフォルト値）
        let initial_server = config.servers.first().map(|s| s.name.as_str());
        let modal = create_reserve_modal(config, None, initial_server, Some(usage_id_str));

        // モーダルを開く
        let session = client.open_session(bot_token);
        let open_view_req = SlackApiViewsOpenRequest::new(trigger_id.clone(), modal);

        session.views_open(&open_view_req).await?;

        info!("✅ 更新モーダルを開きました（予約ID: {}）", usage_id_str);
        Ok(())
    }
}
