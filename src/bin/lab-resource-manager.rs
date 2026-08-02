//! カレンダー共有用 Slack Bot
//!
//! このバイナリは、ユーザーがGmailアカウントを登録し、
//! 共有リソースカレンダーへのアクセス権を取得できるSlack Botを実行します。

use axum_server::tls_rustls::RustlsConfig;
use chrono::Duration as ChronoDuration;
use lab_resource_manager::{
    application::idle_notice_log::IdleNoticeLog,
    application::usecases::{
        accept_reservation_proposal::AcceptReservationProposalUseCase,
        check_resource_availability::CheckResourceAvailabilityUseCase,
        create_resource_usage::CreateResourceUsageUseCase,
        delete_resource_usage::DeleteResourceUsageUseCase,
        describe_monitoring::{DescribeMonitoringUseCase, MonitoringSettings},
        detect_idle_reservations::DetectIdleReservationsUseCase,
        get_resource_usage_by_id::GetResourceUsageByIdUseCase,
        grant_user_resource_access::GrantUserResourceAccessUseCase,
        list_all_future_resource_usages::ListAllFutureResourceUsagesUseCase,
        list_user_resource_usages::ListUserResourceUsagesUseCase,
        notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase,
        reconcile_observed_usages::ReconcileObservedUsagesUseCase,
        release_resource_usage_early::ReleaseResourceUsageEarlyUseCase,
        update_resource_usage::UpdateResourceUsageUseCase,
    },
    domain::ports::ResourceUsageObserver,
    infrastructure::{
        config::{load_config, load_from_env},
        idle_reservation_notifier::SlackIdleReservationNotifier,
        notifier::NotificationRouter,
        repositories::{
            identity_link::JsonFileIdentityLinkRepository, mcp_token::JsonFileMcpTokenRepository,
            resource_usage::google_calendar::GoogleCalendarUsageRepository,
        },
        reservation_proposal::SlackReservationProposalNotifier,
        resource_collection_access::GoogleCalendarAccessService,
        resource_usage_observer::SharedFileResourceUsageObserver,
        unauthorized_usage_notifier::SlackUnauthorizedUsageNotifier,
    },
    interface::mcp::{self, server::LrmMcpServer},
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
    let resource_config = Arc::new(load_config(&app_config.resource_config_path)?);

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

    let resource_usage_repo = Arc::new(
        GoogleCalendarUsageRepository::new(
            service_account_key,
            resource_config.as_ref().clone(),
            app_config.calendar_mappings_file.clone(),
            identity_repo.clone(),
        )
        .await?,
    );

    // UseCases
    let collection_ids: Vec<String> = resource_config
        .servers
        .iter()
        .map(|s| s.calendar_id.clone())
        .chain(resource_config.rooms.iter().map(|r| r.calendar_id.clone()))
        .collect();

    let grant_access_usecase = Arc::new(GrantUserResourceAccessUseCase::new(
        identity_repo.clone(),
        calendar_access_service,
        collection_ids,
    ));

    let create_usecase = Arc::new(CreateResourceUsageUseCase::new(resource_usage_repo.clone()));
    let accept_proposal_usecase = Arc::new(AcceptReservationProposalUseCase::new(
        resource_usage_repo.clone(),
    ));
    let update_usecase = Arc::new(UpdateResourceUsageUseCase::new(resource_usage_repo.clone()));
    let delete_usecase = Arc::new(DeleteResourceUsageUseCase::new(resource_usage_repo.clone()));
    let release_early_usecase = Arc::new(ReleaseResourceUsageEarlyUseCase::new(
        resource_usage_repo.clone(),
    ));
    let check_availability_usecase = Arc::new(CheckResourceAvailabilityUseCase::new(
        resource_usage_repo.clone(),
    ));
    let list_all_usecase = Arc::new(ListAllFutureResourceUsagesUseCase::new(
        resource_usage_repo.clone(),
    ));
    let list_mine_usecase = Arc::new(ListUserResourceUsagesUseCase::new(
        resource_usage_repo.clone(),
    ));
    let get_by_id_usecase = Arc::new(GetResourceUsageByIdUseCase::new(
        resource_usage_repo.clone(),
    ));

    let notifier = NotificationRouter::new(resource_config.as_ref().clone(), identity_repo.clone());
    let notify_usecase = Arc::new(
        NotifyFutureResourceUsageChangesUseCase::new(
            resource_usage_repo.clone(),
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
    let idle_notices = Arc::new(IdleNoticeLog::default());

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
            resource_usage_repo.clone(),
            observer.clone(),
            identity_repo.clone(),
            proposal_notifier,
            unauthorized_notifier,
            ChronoDuration::seconds(app_config.unreserved_usage_threshold_secs as i64),
            app_config.reservation_proposal_duration_candidates.clone(),
        ));

        let idle_notifier = SlackIdleReservationNotifier::new(
            slack_client.clone(),
            SlackApiToken::new(app_config.slack_bot_token.clone().into()),
            identity_repo.clone(),
        );
        let detect_idle_usecase = Arc::new(DetectIdleReservationsUseCase::new(
            resource_usage_repo.clone(),
            observer,
            identity_repo.clone(),
            idle_notifier,
            ChronoDuration::seconds(app_config.idle_reservation_threshold_secs as i64),
            idle_notices.clone(),
        ));

        let interval = std::time::Duration::from_secs(app_config.polling_interval_secs);
        info!(
            interval_secs = app_config.polling_interval_secs,
            "usage observation enabled"
        );
        Some(tokio::spawn(async move {
            loop {
                if let Err(e) = reconcile_usecase.poll_once().await {
                    error!(error = %e, "usage observation polling failed");
                }
                if let Err(e) = detect_idle_usecase.poll_once().await {
                    error!(error = %e, "idle reservation polling failed");
                }
                tokio::time::sleep(interval).await;
            }
        }))
    } else {
        info!("usage observation disabled: GPU_USAGE_REPORTS_DIR is not set");
        None
    };

    // ===========================================
    // MCPサーバー（オプション機能、MCP_LISTEN_ADDR未設定なら無効）
    // ===========================================
    let mcp_handle = if let Some(mcp_listen_addr) = mcp_listen_addr {
        let mcp_server = LrmMcpServer::new(
            create_usecase.clone(),
            update_usecase.clone(),
            delete_usecase.clone(),
            release_early_usecase.clone(),
            list_all_usecase.clone(),
            list_mine_usecase.clone(),
            get_by_id_usecase.clone(),
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
            create_resource_usage: create_usecase,
            accept_reservation_proposal: accept_proposal_usecase,
            update_resource_usage: update_usecase,
            delete_resource_usage: delete_usecase,
            release_resource_usage_early: release_early_usecase,
            check_resource_availability: check_availability_usecase,
            notify_resource_usage_changes: notify_usecase,
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
