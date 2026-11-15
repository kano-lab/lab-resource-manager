//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::idle_notice_log::IdleNoticeLog;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotQuery};
use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::dependencies::{SlackRepositories, SlackUseCases};
use chrono::{DateTime, Utc};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, info, warn};

/// Slackボット
///
/// スラッシュコマンドとボタン・モーダルの操作を受け付ける入口。
/// 予約の監視や通知のような、Slackがなくても続けるべき処理は持たない
/// （それらはコンポジションルートが起動する）。
pub struct SlackApp<R>
where
    R: ReservationRepository + ReservedSlotQuery,
{
    // 設定
    resource_config: Arc<ResourceConfig>,

    // 依存
    usecases: SlackUseCases<R>,
    repositories: SlackRepositories,

    // 予約者の応答で更新される、未使用予約のお知らせの抑制記録
    idle_notices: Arc<IdleNoticeLog>,

    // Slackインフラストラクチャ
    slack_client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,
    /// Socket Modeの接続に使うApp-Level Token（xapp-...）
    app_token: SlackApiToken,

    // 内部状態
    /// このプロセスが動き始めた時刻（稼働時間の起点）
    started_at: DateTime<Utc>,
    user_channel_map: Arc<RwLock<HashMap<SlackUserId, SlackChannelId>>>,
    task_tracker: TaskTracker,
    http_client: reqwest::Client,
}

impl<R> SlackApp<R>
where
    R: ReservationRepository + ReservedSlotQuery + Send + Sync + 'static,
{
    /// 新しいSlackAppを作成
    ///
    /// すべての依存関係をコンストラクタで受け取ります（Dependency Injection）。
    /// 内部状態（user_channel_map, task_tracker, http_client）はコンストラクタ内で生成します。
    pub fn new(
        resource_config: Arc<ResourceConfig>,
        repositories: SlackRepositories,
        usecases: SlackUseCases<R>,
        idle_notices: Arc<IdleNoticeLog>,
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        app_token: SlackApiToken,
    ) -> Self {
        Self {
            resource_config,
            repositories,
            usecases,
            idle_notices,
            slack_client,
            bot_token,
            app_token,
            started_at: Utc::now(),
            user_channel_map: Arc::new(RwLock::new(HashMap::new())),
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Slackからのイベントを受け付ける
    ///
    /// Socket Modeでつなぎ、`shutdown`が取り消されるか接続が終わるまで受け付ける。
    /// 終えるときは、受け付けたイベントの処理が終わるのを待つ。
    pub async fn run(
        self: Arc<Self>,
        shutdown: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            servers = self.resource_config.servers.len(),
            rooms = self.resource_config.rooms.len(),
            "starting slack bot"
        );

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

        socket_mode_listener.listen_for(&self.app_token).await?;
        info!("connected to slack socket mode; accepting commands");

        tokio::select! {
            _ = socket_mode_listener.serve() => {
                info!("socket mode listener ended");
            }
            _ = shutdown.cancelled() => {}
        }

        info!("waiting for slack event handlers to finish");
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
            .get_user_state::<Arc<SlackApp<R>>>()
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
            .get_user_state::<Arc<SlackApp<R>>>()
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

    pub fn repositories(&self) -> &SlackRepositories {
        &self.repositories
    }

    pub fn usecases(&self) -> &SlackUseCases<R> {
        &self.usecases
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
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
