//! カレンダー共有用 Slack Bot
//!
//! このバイナリは、ユーザーがGmailアカウントを登録し、
//! 共有リソースカレンダーへのアクセス権を取得できるSlack Botを実行します。

use lab_resource_manager::{
    application::usecases::{
        create_resource_usage::CreateResourceUsageUseCase,
        delete_resource_usage::DeleteResourceUsageUseCase,
        grant_user_resource_access::GrantUserResourceAccessUseCase,
        notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase,
        update_resource_usage::UpdateResourceUsageUseCase,
    },
    infrastructure::{
        config::{load_config, load_from_env},
        notifier::NotificationRouter,
        repositories::{
            identity_link::JsonFileIdentityLinkRepository,
            resource_usage::google_calendar::GoogleCalendarUsageRepository,
        },
        resource_collection_access::GoogleCalendarAccessService,
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
        NotifyFutureResourceUsageChangesUseCase::new(resource_usage_repo, notifier)
            .await
            .map_err(|e| format!("通知UseCaseの初期化に失敗: {}", e))?,
    );

    // Slackインフラ
    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let bot_token = SlackApiToken::new(app_config.slack_bot_token.clone().into());

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

    Ok(())
}
