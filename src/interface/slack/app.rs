//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::usecases::create_resource_usage::CreateResourceUsageUseCase;
use crate::application::usecases::delete_resource_usage::DeleteResourceUsageUseCase;
use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::application::usecases::notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase;
use crate::application::usecases::update_resource_usage::UpdateResourceUsageUseCase;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::infrastructure::config::{AppConfig, ResourceConfig};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::task::TaskTracker;

/// 依存性注入を備えたSlackアプリケーション
///
/// このBotアプリケーションに必要なすべての依存関係を保持し、
/// `run()`メソッドでアプリケーション全体を実行します。
pub struct SlackApp<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    // 設定
    app_config: AppConfig,
    resource_config: Arc<ResourceConfig>,

    // UseCases
    grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    create_resource_usage_usecase: Arc<CreateResourceUsageUseCase<R>>,
    update_resource_usage_usecase: Arc<UpdateResourceUsageUseCase<R>>,
    delete_usage_usecase: Arc<DeleteResourceUsageUseCase<R>>,
    notify_usecase: Arc<NotifyFutureResourceUsageChangesUseCase<R, N>>,

    // リポジトリ
    identity_repo: Arc<dyn IdentityLinkRepository>,

    // Slackインフラストラクチャ
    slack_client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,

    // 内部状態
    user_channel_map: Arc<RwLock<HashMap<SlackUserId, SlackChannelId>>>,
    task_tracker: TaskTracker,
    http_client: reqwest::Client,
}

impl<R, N> SlackApp<R, N>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    /// 新しいSlackAppを作成
    ///
    /// すべての依存関係をコンストラクタで受け取ります（Dependency Injection）。
    /// 内部状態（user_channel_map, task_tracker, http_client）はコンストラクタ内で生成します。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app_config: AppConfig,
        resource_config: Arc<ResourceConfig>,
        identity_repo: Arc<dyn IdentityLinkRepository>,
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        create_resource_usage_usecase: Arc<CreateResourceUsageUseCase<R>>,
        update_resource_usage_usecase: Arc<UpdateResourceUsageUseCase<R>>,
        delete_usage_usecase: Arc<DeleteResourceUsageUseCase<R>>,
        notify_usecase: Arc<NotifyFutureResourceUsageChangesUseCase<R, N>>,
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
    ) -> Self {
        Self {
            app_config,
            resource_config,
            identity_repo,
            grant_access_usecase,
            create_resource_usage_usecase,
            update_resource_usage_usecase,
            delete_usage_usecase,
            notify_usecase,
            slack_client,
            bot_token,
            user_channel_map: Arc::new(RwLock::new(HashMap::new())),
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// アプリケーションを実行
    ///
    /// Socket Modeリスナーとポーリングタスクを起動し、
    /// Ctrl+Cシグナルまで実行を継続します。
    pub async fn run(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("🤖 Slack Bot を起動しています...");
        println!(
            "📁 リソース設定ファイル: {}",
            self.app_config.resource_config_path.display()
        );
        println!(
            "📁 ID紐付けファイル: {}",
            self.app_config.identity_links_file.display()
        );
        println!(
            "✅ 設定を読み込みました: {} サーバー, {} 部屋",
            self.resource_config.servers.len(),
            self.resource_config.rooms.len()
        );
        println!("✅ Slack App を初期化しました");
        println!("✅ 通知機能を初期化しました");

        println!("🚀 Bot の準備ができました！");
        println!("   /register-calendar <your-email@gmail.com>");
        println!("   /link-user <@slack_user> <email@gmail.com>");
        println!();

        // Socket Mode リスナーの設定
        let socket_mode_callbacks = SlackSocketModeListenerCallbacks::new()
            .with_command_events(Self::handle_command_event)
            .with_interaction_events(Self::handle_interaction_event);

        let slack_client_for_env = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
        let listener_environment = Arc::new(
            SlackClientEventsListenerEnvironment::new(slack_client_for_env)
                .with_user_state(self.clone()),
        );

        let socket_mode_listener = SlackClientSocketModeListener::new(
            &SlackClientSocketModeConfig::new(),
            listener_environment,
            socket_mode_callbacks,
        );

        println!("🔌 Slack Socket Mode に接続しています...");

        let app_token = SlackApiToken::new(self.app_config.slack_app_token.clone().into());
        socket_mode_listener.listen_for(&app_token).await?;

        println!("✅ Slack Socket Mode に接続しました！");
        println!("🎉 Bot がスラッシュコマンドを待機しています");
        println!();

        println!(
            "🔍 カレンダー監視を開始します（間隔: {}秒）",
            self.app_config.polling_interval_secs
        );
        println!();
        println!("Bot を停止するには Ctrl+C を押してください");

        // バックグラウンドでポーリングタスクを実行
        let polling_handle = {
            let notify_usecase = self.notify_usecase.clone();
            let polling_interval = Duration::from_secs(self.app_config.polling_interval_secs);
            tokio::spawn(async move {
                loop {
                    match notify_usecase.poll_once().await {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("❌ ポーリングエラー: {}", e);
                        }
                    }
                    tokio::time::sleep(polling_interval).await;
                }
            })
        };

        // Socket Mode リスナーとポーリングタスクを並行実行
        tokio::select! {
            _ = socket_mode_listener.serve() => {
                println!("\n🔌 Socket Mode リスナーが終了しました");
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n👋 シャットダウンシグナルを受信しました");
            }
        }

        // ポーリングタスクを停止
        polling_handle.abort();

        println!("👋 シャットダウンしています...");
        self.shutdown().await;

        Ok(())
    }

    /// コマンドイベントハンドラ
    async fn handle_command_event(
        event: SlackCommandEvent,
        _client: Arc<SlackHyperClient>,
        state: SlackClientEventsUserState,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        println!("📩 コマンドを受信しました: {}", event.command);

        let app = state
            .read()
            .await
            .get_user_state::<Arc<SlackApp<R, N>>>()
            .ok_or("App の状態が見つかりません")?
            .clone();

        match app.route_slash_command(event).await {
            Ok(response) => {
                println!("✅ コマンドを正常に処理しました");
                Ok(response)
            }
            Err(e) => {
                eprintln!("❌ コマンド処理エラー: {}", e);
                Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new().with_text(format!("エラー: {}", e)),
                ))
            }
        }
    }

    /// インタラクションイベントハンドラ
    async fn handle_interaction_event(
        event: SlackInteractionEvent,
        client: Arc<SlackHyperClient>,
        state: SlackClientEventsUserState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("🔘 インタラクションを受信しました");

        let app = state
            .read()
            .await
            .get_user_state::<Arc<SlackApp<R, N>>>()
            .ok_or("App の状態が見つかりません")?
            .clone();

        // Socket Modeには即座に応答を返すため、処理を非同期タスクでspawn
        tokio::spawn(async move {
            let result = app.route_interaction(event.clone()).await;

            match result {
                Ok(Some(response)) => {
                    println!("📤 ビュー応答を送信中...");

                    let token = &app.bot_token;
                    let session = client.open_session(token);

                    match response {
                        SlackViewSubmissionResponse::Update(update_response) => {
                            if let SlackInteractionEvent::ViewSubmission(vs) = &event {
                                let view_id = &vs.view.state_params.id;
                                let hash = if let SlackView::Modal(modal) = &vs.view.view {
                                    modal.hash.clone()
                                } else {
                                    None
                                };

                                let mut request =
                                    SlackApiViewsUpdateRequest::new(update_response.view);
                                request.view_id = Some(view_id.clone());
                                request.hash = hash;

                                match session.views_update(&request).await {
                                    Ok(_) => println!("✅ ビューを更新しました"),
                                    Err(e) => eprintln!("❌ ビュー更新エラー: {}", e),
                                }
                            }
                        }
                        SlackViewSubmissionResponse::Push(push_response) => {
                            if let SlackInteractionEvent::ViewSubmission(vs) = &event
                                && let Some(trigger_id) = &vs.trigger_id
                            {
                                match session
                                    .views_push(&SlackApiViewsPushRequest::new(
                                        trigger_id.clone(),
                                        push_response.view,
                                    ))
                                    .await
                                {
                                    Ok(_) => println!("✅ ビューをpushしました"),
                                    Err(e) => eprintln!("❌ ビューpushエラー: {}", e),
                                }
                            }
                        }
                        SlackViewSubmissionResponse::Clear(_) => {
                            println!("⚠️ Clear responseは未実装です");
                        }
                        _ => {}
                    }

                    println!("✅ インタラクションを正常に処理しました");
                }
                Ok(None) => {
                    println!("✅ インタラクションを正常に処理しました（応答なし）");
                }
                Err(e) => {
                    eprintln!("❌ インタラクション処理エラー: {}", e);
                }
            }
        });

        Ok(())
    }

    /// すべてのバックグラウンドタスクの完了を待機
    async fn shutdown(&self) {
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }

    // 以下、既存のメソッドで使用されるフィールドへのアクセサ
    pub fn bot_token(&self) -> &SlackApiToken {
        &self.bot_token
    }

    pub fn slack_client(&self) -> &Arc<SlackHyperClient> {
        &self.slack_client
    }

    pub fn resource_config(&self) -> &Arc<ResourceConfig> {
        &self.resource_config
    }

    pub fn identity_repo(&self) -> &Arc<dyn IdentityLinkRepository> {
        &self.identity_repo
    }

    pub fn grant_access_usecase(&self) -> &Arc<GrantUserResourceAccessUseCase> {
        &self.grant_access_usecase
    }

    pub fn create_resource_usage_usecase(&self) -> &Arc<CreateResourceUsageUseCase<R>> {
        &self.create_resource_usage_usecase
    }

    pub fn update_resource_usage_usecase(&self) -> &Arc<UpdateResourceUsageUseCase<R>> {
        &self.update_resource_usage_usecase
    }

    pub fn delete_usage_usecase(&self) -> &Arc<DeleteResourceUsageUseCase<R>> {
        &self.delete_usage_usecase
    }

    pub fn user_channel_map(&self) -> &Arc<RwLock<HashMap<SlackUserId, SlackChannelId>>> {
        &self.user_channel_map
    }

    pub fn task_tracker(&self) -> &TaskTracker {
        &self.task_tracker
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }
}
