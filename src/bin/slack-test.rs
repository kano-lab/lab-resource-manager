//! Slack通知テスト用バイナリ
//!
//! Google Calendar なしでSlack通知のみをテストするためのスタンドアロンバイナリ。
//! MockリポジトリとMock Access Serviceを使用して、Slackボットの動作を確認できる。

use lab_resource_manager::{
    application::usecases::{
        create_resource_usage::CreateResourceUsageUseCase,
        delete_resource_usage::DeleteResourceUsageUseCase,
        notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase,
        update_resource_usage::UpdateResourceUsageUseCase,
    },
    domain::{
        common::EmailAddress,
        ports::resource_collection_access::{
            ResourceCollectionAccessError, ResourceCollectionAccessService,
        },
    },
    infrastructure::{
        config::load_config,
        notifier::NotificationRouter,
        repositories::identity_link::JsonFileIdentityLinkRepository,
    },
    interface::slack::SlackApp,
    MockUsageRepository,
};
use async_trait::async_trait;
use slack_morphism::prelude::*;
use std::{env, path::PathBuf, sync::Arc};

/// Mock実装: Google Calendar操作をスキップする
struct MockCalendarAccessService;

#[async_trait]
impl ResourceCollectionAccessService for MockCalendarAccessService {
    async fn grant_access(
        &self,
        _collection_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError> {
        println!(
            "📅 [Mock] カレンダーアクセス権付与: {} (実際のAPIコールはスキップ)",
            email.as_str()
        );
        Ok(())
    }

    async fn revoke_access(
        &self,
        _collection_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError> {
        println!(
            "📅 [Mock] カレンダーアクセス権剥奪: {} (実際のAPIコールはスキップ)",
            email.as_str()
        );
        Ok(())
    }
}

/// 設定を環境変数から読み込み（Google関連は不要）
fn load_test_config() -> Result<TestConfig, Box<dyn std::error::Error>> {
    let slack_bot_token = env::var("SLACK_BOT_TOKEN")
        .map_err(|_| "環境変数 SLACK_BOT_TOKEN が必要です")?;

    let slack_app_token = env::var("SLACK_APP_TOKEN")
        .map_err(|_| "環境変数 SLACK_APP_TOKEN が必要です")?;

    let resource_config_path = env::var("RESOURCE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./config/resources.toml"));

    let identity_links_file = env::var("IDENTITY_LINKS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/identity_links.json"));

    let polling_interval_secs = env::var("POLLING_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    Ok(TestConfig {
        slack_bot_token,
        slack_app_token,
        resource_config_path,
        identity_links_file,
        polling_interval_secs,
    })
}

struct TestConfig {
    slack_bot_token: String,
    slack_app_token: String,
    resource_config_path: PathBuf,
    identity_links_file: PathBuf,
    polling_interval_secs: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // トレーシングの初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // rustls暗号化プロバイダの初期化
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    println!("🧪 Slack通知テストモードで起動します");
    println!("   Google Calendar: Mock（実際のAPIコールなし）");
    println!();

    // 設定の読み込み
    let test_config = load_test_config()?;
    let resource_config = Arc::new(load_config(&test_config.resource_config_path)?);

    println!("📁 設定ファイル: {:?}", test_config.resource_config_path);
    println!("📁 IDリンクファイル: {:?}", test_config.identity_links_file);
    println!();

    // リポジトリ（Mock使用）
    let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(
        test_config.identity_links_file.clone(),
    ));
    let resource_usage_repo = Arc::new(MockUsageRepository::new());
    let calendar_access_service = Arc::new(MockCalendarAccessService);

    // UseCases
    let collection_ids: Vec<String> = resource_config
        .servers
        .iter()
        .map(|s| s.calendar_id.clone())
        .chain(resource_config.rooms.iter().map(|r| r.calendar_id.clone()))
        .collect();

    // Note: GrantUserResourceAccessUseCase は直接使えないので、SlackApp用にダミーを作る必要がある
    // ここではSlackAppの構造を変更するか、テスト用の構造を作る

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
    let bot_token = SlackApiToken::new(test_config.slack_bot_token.clone().into());

    // GrantUserResourceAccessUseCaseの作成
    use lab_resource_manager::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
    let grant_access_usecase = Arc::new(GrantUserResourceAccessUseCase::new(
        identity_repo.clone(),
        calendar_access_service,
        collection_ids,
    ));

    // AppConfigを構築
    use lab_resource_manager::infrastructure::config::AppConfig;
    let app_config = AppConfig {
        google_service_account_key_path: PathBuf::from("/dev/null"), // 使用しない
        slack_bot_token: test_config.slack_bot_token,
        slack_app_token: test_config.slack_app_token,
        resource_config_path: test_config.resource_config_path,
        identity_links_file: test_config.identity_links_file,
        calendar_mappings_file: PathBuf::from("/dev/null"), // 使用しない
        polling_interval_secs: test_config.polling_interval_secs,
    };

    // アプリケーションの組み立てと実行
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

    println!("🚀 Slackボットを起動します（Socket Mode）");
    println!("   Ctrl+C で終了");
    println!();

    app.run()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })?;

    Ok(())
}
