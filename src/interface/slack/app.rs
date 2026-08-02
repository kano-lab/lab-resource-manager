//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::idle_notice_log::IdleNoticeLog;
use crate::application::usecases::accept_reservation_proposal::AcceptReservationProposalUseCase;
use crate::application::usecases::check_resource_availability::CheckResourceAvailabilityUseCase;
use crate::application::usecases::create_resource_usage::CreateResourceUsageUseCase;
use crate::application::usecases::delete_resource_usage::DeleteResourceUsageUseCase;
use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::application::usecases::notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase;
use crate::application::usecases::release_resource_usage_early::ReleaseResourceUsageEarlyUseCase;
use crate::application::usecases::update_resource_usage::UpdateResourceUsageUseCase;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, McpTokenRepository, ResourceUsageRepository,
};
use crate::infrastructure::config::{AppConfig, ResourceConfig};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, info, warn};

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
    accept_reservation_proposal_usecase: Arc<AcceptReservationProposalUseCase<R>>,
    update_resource_usage_usecase: Arc<UpdateResourceUsageUseCase<R>>,
    delete_usage_usecase: Arc<DeleteResourceUsageUseCase<R>>,
    release_early_usecase: Arc<ReleaseResourceUsageEarlyUseCase<R>>,
    check_availability_usecase: Arc<CheckResourceAvailabilityUseCase<R>>,
    notify_usecase: Arc<NotifyFutureResourceUsageChangesUseCase<R, N>>,

    // 予約者の応答で更新される、未使用予約のお知らせの抑制記録
    idle_notices: Arc<IdleNoticeLog>,

    // リポジトリ
    identity_repo: Arc<dyn IdentityLinkRepository>,
    mcp_token_repo: Arc<dyn McpTokenRepository>,

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
        mcp_token_repo: Arc<dyn McpTokenRepository>,
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        create_resource_usage_usecase: Arc<CreateResourceUsageUseCase<R>>,
        accept_reservation_proposal_usecase: Arc<AcceptReservationProposalUseCase<R>>,
        update_resource_usage_usecase: Arc<UpdateResourceUsageUseCase<R>>,
        delete_usage_usecase: Arc<DeleteResourceUsageUseCase<R>>,
        release_early_usecase: Arc<ReleaseResourceUsageEarlyUseCase<R>>,
        check_availability_usecase: Arc<CheckResourceAvailabilityUseCase<R>>,
        notify_usecase: Arc<NotifyFutureResourceUsageChangesUseCase<R, N>>,
        idle_notices: Arc<IdleNoticeLog>,
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
    ) -> Self {
        Self {
            app_config,
            resource_config,
            identity_repo,
            mcp_token_repo,
            grant_access_usecase,
            create_resource_usage_usecase,
            accept_reservation_proposal_usecase,
            update_resource_usage_usecase,
            delete_usage_usecase,
            release_early_usecase,
            check_availability_usecase,
            notify_usecase,
            idle_notices,
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
        info!(
            resource_config = %self.app_config.resource_config_path.display(),
            identity_links = %self.app_config.identity_links_file.display(),
            servers = self.resource_config.servers.len(),
            rooms = self.resource_config.rooms.len(),
            "starting slack bot"
        );

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

        let app_token = SlackApiToken::new(self.app_config.slack_app_token.clone().into());
        socket_mode_listener.listen_for(&app_token).await?;

        info!(
            polling_interval_secs = self.app_config.polling_interval_secs,
            "connected to slack socket mode; accepting commands"
        );

        // バックグラウンドでポーリングタスクを実行
        let polling_handle = {
            let notify_usecase = self.notify_usecase.clone();
            let polling_interval = Duration::from_secs(self.app_config.polling_interval_secs);
            tokio::spawn(async move {
                loop {
                    match notify_usecase.poll_once().await {
                        Ok(_) => {}
                        Err(e) => {
                            error!(error = %e, "reservation change polling failed");
                        }
                    }
                    tokio::time::sleep(polling_interval).await;
                }
            })
        };

        // Socket Mode リスナーとポーリングタスクを並行実行
        tokio::select! {
            _ = socket_mode_listener.serve() => {
                info!("socket mode listener ended");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received shutdown signal");
            }
        }

        // ポーリングタスクを停止
        polling_handle.abort();

        info!("shutting down");
        self.shutdown().await;

        Ok(())
    }

    /// コマンドイベントハンドラ
    async fn handle_command_event(
        event: SlackCommandEvent,
        _client: Arc<SlackHyperClient>,
        state: SlackClientEventsUserState,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        // ハンドラの結果ログでも使うため、イベントを渡す前に控えておく
        let command = event.command.to_string();
        debug!(command = %command, "received slash command");

        let app = state
            .read()
            .await
            .get_user_state::<Arc<SlackApp<R, N>>>()
            .ok_or("App の状態が見つかりません")?
            .clone();

        match app.route_slash_command(event).await {
            Ok(response) => {
                debug!("slash command handled");
                Ok(response)
            }
            Err(e) => {
                error!(command = %command, error = %e, "slash command failed");
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
        debug!("received interaction");

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
                    debug!("sending view response");

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
                                    Ok(_) => debug!("view updated"),
                                    Err(e) => error!(error = %e, "updating the view failed"),
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
                                    Ok(_) => debug!("view pushed"),
                                    Err(e) => error!(error = %e, "pushing the view failed"),
                                }
                            }
                        }
                        SlackViewSubmissionResponse::Clear(_) => {
                            warn!("clear response is not implemented");
                        }
                        _ => {}
                    }

                    debug!("interaction handled");
                }
                Ok(None) => {
                    debug!("interaction handled without a response");
                }
                Err(e) => {
                    error!(error = %e, "interaction failed");
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

    pub fn mcp_token_repo(&self) -> &Arc<dyn McpTokenRepository> {
        &self.mcp_token_repo
    }

    pub fn grant_access_usecase(&self) -> &Arc<GrantUserResourceAccessUseCase> {
        &self.grant_access_usecase
    }

    pub fn create_resource_usage_usecase(&self) -> &Arc<CreateResourceUsageUseCase<R>> {
        &self.create_resource_usage_usecase
    }

    pub fn accept_reservation_proposal_usecase(&self) -> &Arc<AcceptReservationProposalUseCase<R>> {
        &self.accept_reservation_proposal_usecase
    }

    pub fn update_resource_usage_usecase(&self) -> &Arc<UpdateResourceUsageUseCase<R>> {
        &self.update_resource_usage_usecase
    }

    pub fn delete_usage_usecase(&self) -> &Arc<DeleteResourceUsageUseCase<R>> {
        &self.delete_usage_usecase
    }

    pub fn release_early_usecase(&self) -> &Arc<ReleaseResourceUsageEarlyUseCase<R>> {
        &self.release_early_usecase
    }

    pub fn check_availability_usecase(&self) -> &Arc<CheckResourceAvailabilityUseCase<R>> {
        &self.check_availability_usecase
    }

    pub fn idle_notices(&self) -> &Arc<IdleNoticeLog> {
        &self.idle_notices
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
