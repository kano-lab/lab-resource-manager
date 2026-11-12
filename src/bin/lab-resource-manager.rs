//! カレンダー共有用 Slack Bot
//!
//! このバイナリは、ユーザーがGmailアカウントを登録し、
//! 共有リソースカレンダーへのアクセス権を取得できるSlack Botを実行します。

use axum_server::tls_rustls::RustlsConfig;
use chrono::Duration as ChronoDuration;
use lab_resource_manager::{
    application::idle_notice_log::IdleNoticeLog,
    application::usecases::{
        AcceptReservationProposalUseCase, CancelSlotUseCase, CheckResourceAvailabilityUseCase,
        CreateReservationUseCase, DetectIdleSlotsUseCase, EndSlotEarlyUseCase, GetSlotUseCase,
        GrantUserResourceAccessUseCase, IdleCriteria, ListSlotsHeldByUseCase, ListSlotsUseCase,
        NotifyReservationChangesUseCase, ReconcileObservedUsagesUseCase, UpdateSlotUseCase,
        describe_monitoring::{DescribeMonitoringUseCase, MonitoringSettings},
    },
    domain::ports::ResourceUsageObserver,
    infrastructure::{
        config::{load_config, load_from_env},
        idle_slot_notifier::SlackIdleSlotNotifier,
        notifier::NotificationRouter,
        repositories::{
            identity_link::JsonFileIdentityLinkRepository, mcp_token::JsonFileMcpTokenRepository,
            reservation::google_calendar::GoogleCalendarReservationRepository,
        },
        reservation_proposal::SlackReservationProposalNotifier,
        resource_collection_access::GoogleCalendarAccessService,
        resource_usage_observer::SharedFileResourceUsageObserver,
        unauthorized_usage_notifier::SlackUnauthorizedUsageNotifier,
    },
    interface::mcp::{
        self,
        server::{LrmMcpServer, McpUseCases},
    },
    interface::slack::{SlackApp, SlackRepositories, SlackUseCases},
};
use slack_morphism::prelude::*;
use std::sync::Arc;
use tracing::{error, info};

/// 予約の変更を監視する期間の長さ
///
/// 前回と今回の状態を突き合わせて差分を通知するため、この長さを縮めると
/// 範囲外に出た予約が削除として通知される。繰り返し予約は個々の予定へ展開されるので、
/// 上限なしにはできない。
const NOTIFICATION_WATCH_WINDOW_DAYS: i64 = 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tracingの初期化（RUST_LOGで制御、未設定時はinfo）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // rustls暗号化プロバイダの初期化
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // ===========================================
    // 設定の読み込み
    // ===========================================
    let app_config = load_from_env()?;
    let (resource_config, storage_config) = load_config(&app_config.resource_config_path)?;
    let resource_config = Arc::new(resource_config);

    // 後段でapp_configがSlackAppに所有権移動するため、MCP関連の値は先に控えておく
    let mcp_listen_addr = app_config.mcp_listen_addr;
    let mcp_allowed_hosts = app_config.mcp_allowed_hosts.clone();

    // TLS証明書は起動時に読み込む(fail-fast: 壊れた証明書に気づかずHTTPへ
    // 静かにフォールバックする方が危険なため)。両方Someか両方Noneかは
    // loader.rsで検証済み
    let mcp_tls_config = match (&app_config.mcp_tls_cert_file, &app_config.mcp_tls_key_file) {
        (Some(cert), Some(key)) => Some(
            RustlsConfig::from_pem_file(cert, key)
                .await
                .map_err(|e| format!("MCPサーバーのTLS証明書の読み込みに失敗: {}", e))?,
        ),
        _ => None,
    };

    let service_account_key = app_config
        .google_service_account_key_path
        .to_str()
        .ok_or("サービスアカウントキーパスが不正なUTF-8です")?;

    // ===========================================
    // 依存の組み立て（コンポジションルート）
    // ===========================================

    // リポジトリ
    let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(
        app_config.identity_links_file.clone(),
    ));

    let mcp_token_repo = Arc::new(JsonFileMcpTokenRepository::new(
        app_config.mcp_tokens_file.clone(),
    ));

    let calendar_access_service =
        Arc::new(GoogleCalendarAccessService::new(service_account_key).await?);

    let reservation_repo = Arc::new(
        GoogleCalendarReservationRepository::new(
            service_account_key,
            resource_config.as_ref().clone(),
            identity_repo.clone(),
            storage_config.clone(),
        )
        .await?,
    );

    // UseCases
    // ストレージ設定からカレンダーIDのマッピングを取得
    let collection_ids: Vec<String> = storage_config
        .google_calendar_mappings()
        .ok_or("Google Calendarバックエンドの設定が見つかりません")?
        .values()
        .cloned()
        .collect();

    let grant_access_usecase = Arc::new(GrantUserResourceAccessUseCase::new(
        identity_repo.clone(),
        calendar_access_service,
        collection_ids,
    ));

    let create_reservation_usecase =
        Arc::new(CreateReservationUseCase::new(reservation_repo.clone()));
    let accept_proposal_usecase = Arc::new(AcceptReservationProposalUseCase::new(
        reservation_repo.clone(),
    ));
    let update_slot_usecase = Arc::new(UpdateSlotUseCase::new(reservation_repo.clone()));
    let cancel_slot_usecase = Arc::new(CancelSlotUseCase::new(reservation_repo.clone()));
    let end_slot_early_usecase = Arc::new(EndSlotEarlyUseCase::new(reservation_repo.clone()));
    let check_availability_usecase = Arc::new(CheckResourceAvailabilityUseCase::new(
        reservation_repo.clone(),
    ));

    let notifier = NotificationRouter::new(resource_config.as_ref().clone(), identity_repo.clone());
    let notify_usecase = Arc::new(
        NotifyReservationChangesUseCase::new(
            reservation_repo.clone(),
            notifier,
            ChronoDuration::days(NOTIFICATION_WATCH_WINDOW_DAYS),
        )
        .await
        .map_err(|e| format!("通知UseCaseの初期化に失敗: {}", e))?,
    );

    // Slackインフラ
    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let bot_token = SlackApiToken::new(app_config.slack_bot_token.clone().into());

    // ===========================================
    // 実利用観測（オプション機能、GPU_USAGE_REPORTS_DIR未設定なら無効）
    // ===========================================

    // 予約者が「まだ使う」と答えたことを観測側に伝えるため、Slackと観測で同じ記録を共有する
    let idle_notices = Arc::new(IdleNoticeLog::new(ChronoDuration::seconds(
        app_config.idle_notice_silence_secs as i64,
    )));

    let max_staleness = ChronoDuration::seconds(app_config.gpu_usage_max_staleness_secs as i64);
    let observer = app_config.gpu_usage_reports_dir.clone().map(|dir| {
        Arc::new(SharedFileResourceUsageObserver::new(
            dir,
            resource_config.clone(),
            max_staleness,
        ))
    });

    let describe_monitoring_usecase = Arc::new(DescribeMonitoringUseCase::new(
        observer
            .clone()
            .map(|observer| observer as Arc<dyn ResourceUsageObserver>),
        MonitoringSettings {
            polling_interval: ChronoDuration::seconds(app_config.polling_interval_secs as i64),
            idle_threshold: ChronoDuration::seconds(
                app_config.idle_reservation_threshold_secs as i64,
            ),
            held_gpu_threshold: ChronoDuration::seconds(
                app_config.idle_held_gpu_threshold_secs as i64,
            ),
            max_staleness,
        },
    ));

    let reconcile_handle = if let Some(observer) = observer {
        let proposal_notifier = SlackReservationProposalNotifier::new(
            slack_client.clone(),
            SlackApiToken::new(app_config.slack_bot_token.clone().into()),
            identity_repo.clone(),
        );
        let unauthorized_notifier = SlackUnauthorizedUsageNotifier::new(
            slack_client.clone(),
            SlackApiToken::new(app_config.slack_bot_token.clone().into()),
            identity_repo.clone(),
        );

        let reconcile_usecase = Arc::new(ReconcileObservedUsagesUseCase::new(
            reservation_repo.clone(),
            observer.clone(),
            identity_repo.clone(),
            proposal_notifier,
            unauthorized_notifier,
            ChronoDuration::seconds(app_config.unreserved_usage_threshold_secs as i64),
            app_config.reservation_proposal_duration_candidates.clone(),
        ));

        let idle_notifier = SlackIdleSlotNotifier::new(
            slack_client.clone(),
            SlackApiToken::new(app_config.slack_bot_token.clone().into()),
            identity_repo.clone(),
        );
        let detect_idle_usecase = Arc::new(DetectIdleSlotsUseCase::new(
            reservation_repo.clone(),
            observer,
            idle_notifier,
            IdleCriteria {
                absent_threshold: ChronoDuration::seconds(
                    app_config.idle_reservation_threshold_secs as i64,
                ),
                held_threshold: ChronoDuration::seconds(
                    app_config.idle_held_gpu_threshold_secs as i64,
                ),
                computing_utilization_percent: app_config.computing_gpu_utilization_percent,
                held_notices: app_config.idle_held_gpu_notices,
                observation_gap_tolerance: max_staleness,
            },
            idle_notices.clone(),
        ));

        let interval = std::time::Duration::from_secs(app_config.polling_interval_secs);
        info!(
            interval_secs = app_config.polling_interval_secs,
            "slot observation enabled"
        );
        Some(tokio::spawn(async move {
            loop {
                if let Err(e) = reconcile_usecase.poll_once().await {
                    error!(error = %e, "slot observation polling failed");
                }
                if let Err(e) = detect_idle_usecase.poll_once().await {
                    error!(error = %e, "idle reservation polling failed");
                }
                tokio::time::sleep(interval).await;
            }
        }))
    } else {
        info!("slot observation disabled: GPU_USAGE_REPORTS_DIR is not set");
        None
    };

    // ===========================================
    // MCPサーバー（オプション機能、MCP_LISTEN_ADDR未設定なら無効）
    // ===========================================
    let mcp_handle = if let Some(mcp_listen_addr) = mcp_listen_addr {
        let mcp_server = LrmMcpServer::new(
            McpUseCases {
                create_reservation: create_reservation_usecase.clone(),
                list_slots: Arc::new(ListSlotsUseCase::new(reservation_repo.clone())),
                list_slots_held_by: Arc::new(ListSlotsHeldByUseCase::new(reservation_repo.clone())),
                get_slot: Arc::new(GetSlotUseCase::new(reservation_repo.clone())),
                update_slot: update_slot_usecase.clone(),
                end_slot_early: end_slot_early_usecase.clone(),
                cancel_slot: cancel_slot_usecase.clone(),
                check_resource_availability: check_availability_usecase.clone(),
            },
            resource_config.clone(),
        );
        let mcp_token_repo_for_serve = mcp_token_repo.clone();

        info!(listen_addr = %mcp_listen_addr, "mcp server enabled");
        Some(tokio::spawn(async move {
            if let Err(e) = mcp::serve(
                mcp_listen_addr,
                mcp_allowed_hosts,
                mcp_tls_config,
                mcp_token_repo_for_serve,
                mcp_server,
            )
            .await
            {
                error!(error = %e, "mcp server stopped with an error");
            }
        }))
    } else {
        info!("mcp server disabled: MCP_LISTEN_ADDR is not set");
        None
    };

    // ===========================================
    // アプリケーションの組み立てと実行
    // ===========================================
    let app = Arc::new(SlackApp::new(
        app_config,
        resource_config,
        SlackRepositories {
            identity_link: identity_repo,
            mcp_token: mcp_token_repo.clone(),
        },
        SlackUseCases {
            grant_access: grant_access_usecase,
            create_reservation: create_reservation_usecase,
            accept_reservation_proposal: accept_proposal_usecase,
            update_slot: update_slot_usecase,
            cancel_slot: cancel_slot_usecase,
            end_slot_early: end_slot_early_usecase,
            check_resource_availability: check_availability_usecase,
            notify_reservation_changes: notify_usecase,
            describe_monitoring: describe_monitoring_usecase,
        },
        idle_notices,
        slack_client,
        bot_token,
    ));

    app.run()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })?;

    if let Some(handle) = reconcile_handle {
        handle.abort();
    }

    if let Some(handle) = mcp_handle {
        handle.abort();
    }

    Ok(())
}
