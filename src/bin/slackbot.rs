//! カレンダー共有用 Slack Bot
//!
//! このバイナリは、ユーザーがGmailアカウントを登録し、
//! 共有リソースカレンダーへのアクセス権を取得できるSlack Botを実行します。
//!
//! ## 使い方
//!
//! ```bash
//! # 環境変数を指定して実行
//! SLACK_BOT_TOKEN=xoxb-... \
//! GOOGLE_SERVICE_ACCOUNT_KEY=/path/to/key.json \
//! IDENTITY_LINKS_FILE=/path/to/identity_links.json \
//! cargo run --bin slackbot
//! ```
//!
//! ## 環境変数
//!
//! - `SLACK_BOT_TOKEN`: Slack Bot User OAuth Token (必須, xoxb-...)
//! - `SLACK_APP_TOKEN`: Socket Mode用のSlack App-Level Token (必須, xapp-...)
//! - `GOOGLE_SERVICE_ACCOUNT_KEY`: Google サービスアカウントJSONキーのパス (必須)
//! - `RESOURCE_CONFIG`: リソース設定ファイルのパス (デフォルト: config/resources.toml)
//! - `IDENTITY_LINKS_FILE`: ID紐付けJSONファイルのパス (デフォルト: data/identity_links.json)
use lab_resource_manager::{
    application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase,
    infrastructure::{
        config::load_config, repositories::identity_link::JsonFileIdentityLinkRepository,
        resource_collection_access::GoogleCalendarAccessService,
    },
    interface::slack::{SlackBot, SlackCommandHandler},
};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: rustls暗号化プロバイダの初期化
    // google-calendar3クレートが内部でhyper-rustlsを使用しており、
    // rustls 0.23以降ではプロセスレベルでCryptoProviderを明示的に設定する必要がある。
    // これを行わないと "no process-level CryptoProvider available" エラーが発生する。
    // 詳細: https://docs.rs/rustls/latest/rustls/crypto/struct.CryptoProvider.html
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // 環境変数の読み込み
    dotenv::dotenv().ok();

    let service_account_key = env::var("GOOGLE_SERVICE_ACCOUNT_KEY")
        .expect("環境変数 GOOGLE_SERVICE_ACCOUNT_KEY が必要です");

    // デフォルト値を持つオプションの環境変数
    let resource_config_path =
        env::var("RESOURCE_CONFIG").unwrap_or_else(|_| "config/resources.toml".to_string());

    let identity_links_file =
        env::var("IDENTITY_LINKS_FILE").unwrap_or_else(|_| "data/identity_links.json".to_string());

    println!("🤖 Slack Bot を起動しています...");
    println!("📁 リソース設定ファイル: {}", resource_config_path);
    println!("📁 ID紐付けファイル: {}", identity_links_file);

    // 設定の読み込み
    let config = load_config(&resource_config_path)?;
    println!(
        "✅ 設定を読み込みました: {} サーバー, {} 部屋",
        config.servers.len(),
        config.rooms.len()
    );

    // インフラストラクチャの初期化
    let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(PathBuf::from(
        identity_links_file,
    )));

    let calendar_service = Arc::new(GoogleCalendarAccessService::new(&service_account_key).await?);
    println!("✅ Google Calendar サービスを初期化しました");

    // ユースケースの作成
    // すべてのリソースコレクションIDを収集
    let collection_ids: Vec<String> = config
        .servers
        .iter()
        .map(|s| s.calendar_id.clone())
        .chain(config.rooms.iter().map(|r| r.calendar_id.clone()))
        .collect();

    let grant_access_usecase = Arc::new(GrantUserResourceAccessUseCase::new(
        identity_repo.clone(),
        calendar_service,
        collection_ids,
    ));

    // コマンドハンドラとBotの作成
    let command_handler = Arc::new(SlackCommandHandler::new(grant_access_usecase));

    let bot = Arc::new(
        SlackBot::new(command_handler)
            .await
            .map_err(|e| format!("Slack Bot の作成に失敗しました: {}", e))?,
    );
    println!("✅ Slack Bot を初期化しました");

    // Socket Modeのセットアップ
    let app_token =
        env::var("SLACK_APP_TOKEN").expect("Socket Mode には環境変数 SLACK_APP_TOKEN が必要です");

    println!("🚀 Bot の準備ができました！");
    println!("   /register-calendar <your-email@gmail.com>");
    println!("   /link-user <@slack_user> <email@gmail.com>");
    println!();

    // Socket Mode リスナーの作成
    use slack_morphism::prelude::*;

    // コマンドハンドラ関数
    async fn handle_command_event(
        event: SlackCommandEvent,
        _client: Arc<SlackHyperClient>,
        state: SlackClientEventsUserState,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        println!("📩 コマンドを受信しました: {}", event.command);

        // Botを状態から取得
        let bot = state
            .read()
            .await
            .get_user_state::<Arc<SlackBot>>()
            .ok_or("Bot の状態が見つかりません")?
            .clone();

        match bot.handle_command(event).await {
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

    let socket_mode_callbacks =
        SlackSocketModeListenerCallbacks::new().with_command_events(handle_command_event);

    let listener_environment = Arc::new(
        SlackClientEventsListenerEnvironment::new(bot.client()).with_user_state(bot.clone()),
    );

    let socket_mode_listener = SlackClientSocketModeListener::new(
        &SlackClientSocketModeConfig::new(),
        listener_environment.clone(),
        socket_mode_callbacks,
    );

    println!("🔌 Slack Socket Mode に接続しています...");

    socket_mode_listener
        .listen_for(&SlackApiToken::new(app_token.into()))
        .await?;

    println!("✅ Slack Socket Mode に接続しました！");
    println!("🎉 Bot がスラッシュコマンドを待機しています");
    println!();
    println!("Bot を停止するには Ctrl+C を押してください");

    // プロセスを実行し続ける
    socket_mode_listener.serve().await;

    println!("\n👋 シャットダウンしています...");

    Ok(())
}
