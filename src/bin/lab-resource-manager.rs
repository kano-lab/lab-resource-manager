//! カレンダー共有用 Slack Bot
//!
//! このバイナリは、ユーザーがGmailアカウントを登録し、
//! 共有リソースカレンダーへのアクセス権を取得できるSlack Botを実行します。

use chrono::Duration as ChronoDuration;
use lab_resource_manager::{
    application::usecases::{
        create_resource_usage::CreateResourceUsageUseCase,
        delete_resource_usage::DeleteResourceUsageUseCase,
        grant_user_resource_access::GrantUserResourceAccessUseCase,
        notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase,
        reconcile_observed_usages::ReconcileObservedUsagesUseCase,
        update_resource_usage::UpdateResourceUsageUseCase,
    },
    infrastructure::{
        config::{load_config, load_from_env},
        notifier::NotificationRouter,
        repositories::{
            identity_link::JsonFileIdentityLinkRepository,
            resource_usage::google_calendar::GoogleCalendarUsageRepository,
        },
        reservation_proposal::SlackReservationProposalNotifier,
        resource_collection_access::GoogleCalendarAccessService,
        resource_usage_observer::SharedFileResourceUsageObserver,
        unauthorized_usage_notifier::SlackUnauthorizedUsageNotifier,
    },
    interface::slack::SlackApp,
};
use slack_morphism::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // rustls暗号化プロバイダの初期化
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // ===========================================
    // 設定の読み込み
    // ===========================================
    let app_config = load_from_env()?;
    let resource_config = Arc::new(load_config(&app_config.resource_config_path)?);

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

    let calendar_access_service =
        Arc::new(GoogleCalendarAccessService::new(service_account_key).await?);

    let resource_usage_repo = Arc::new(
        GoogleCalendarUsageRepository::new(
            service_account_key,
            resource_config.as_ref().clone(),
            app_config.calendar_mappings_file.clone(),
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
    let update_usecase = Arc::new(UpdateResourceUsageUseCase::new(resource_usage_repo.clone()));
    let delete_usecase = Arc::new(DeleteResourceUsageUseCase::new(resource_usage_repo.clone()));

    let notifier = NotificationRouter::new(resource_config.as_ref().clone(), identity_repo.clone());
    let notify_usecase = Arc::new(
        NotifyFutureResourceUsageChangesUseCase::new(resource_usage_repo.clone(), notifier)
            .await
            .map_err(|e| format!("通知UseCaseの初期化に失敗: {}", e))?,
    );

    // Slackインフラ
    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let bot_token = SlackApiToken::new(app_config.slack_bot_token.clone().into());

    // ===========================================
    // 実利用観測（オプション機能、GPU_USAGE_REPORTS_DIR未設定なら無効）
    // ===========================================
    let reconcile_handle =
        if let Some(gpu_usage_reports_dir) = app_config.gpu_usage_reports_dir.clone() {
            let observer = Arc::new(SharedFileResourceUsageObserver::new(
                gpu_usage_reports_dir,
                resource_config.clone(),
                ChronoDuration::seconds(app_config.gpu_usage_max_staleness_secs as i64),
            ));
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
                observer,
                identity_repo.clone(),
                proposal_notifier,
                unauthorized_notifier,
                ChronoDuration::seconds(app_config.unreserved_usage_threshold_secs as i64),
                app_config.reservation_proposal_duration_candidates.clone(),
            ));

            let interval = std::time::Duration::from_secs(app_config.polling_interval_secs);
            println!("🔍 実利用観測機能を有効化しました");
            Some(tokio::spawn(async move {
                loop {
                    if let Err(e) = reconcile_usecase.poll_once().await {
                        eprintln!("❌ 実利用観測ポーリングエラー: {}", e);
                    }
                    tokio::time::sleep(interval).await;
                }
            }))
        } else {
            println!("ℹ️  GPU_USAGE_REPORTS_DIR未設定のため、実利用観測機能は無効です");
            None
        };

    // ===========================================
    // アプリケーションの組み立てと実行
    // ===========================================
    let app = Arc::new(SlackApp::new(
        app_config,
        resource_config,
        identity_repo,
        grant_access_usecase,
        create_usecase,
        update_usecase,
        delete_usecase,
        notify_usecase,
        slack_client,
        bot_token,
    ));

    app.run()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })?;

    if let Some(handle) = reconcile_handle {
        handle.abort();
    }

    Ok(())
}
