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
use std::sync::Arc;
use std::time::Duration;

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

    // アプリケーション設定の読み込み
    let app_config = load_from_env()?;

    println!("🤖 Slack Bot を起動しています...");
    println!(
        "📁 リソース設定ファイル: {}",
        app_config.resource_config_path.display()
    );
    println!(
        "📁 ID紐付けファイル: {}",
        app_config.identity_links_file.display()
    );

    // リソース設定の読み込み
    let config = load_config(&app_config.resource_config_path)?;
    println!(
        "✅ 設定を読み込みました: {} サーバー, {} 部屋",
        config.servers.len(),
        config.rooms.len()
    );

    // サービスアカウントキーパスの検証
    let service_account_key_path = app_config
        .google_service_account_key_path
        .to_str()
        .ok_or("サービスアカウントキーパスが不正なUTF-8です")?;

    // インフラストラクチャの初期化
    let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(
        app_config.identity_links_file.clone(),
    ));

    let calendar_service =
        Arc::new(GoogleCalendarAccessService::new(service_account_key_path).await?);
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
    let config_arc = Arc::new(config);

    // リソース使用予定リポジトリの作成（予約機能用）
    let resource_usage_repo = Arc::new(
        GoogleCalendarUsageRepository::new(
            service_account_key_path,
            config_arc.as_ref().clone(),
            app_config.calendar_mappings_file.clone(),
        )
        .await?,
    );

    // リソース使用予定UseCasesの作成
    let create_resource_usage_usecase =
        Arc::new(CreateResourceUsageUseCase::new(resource_usage_repo.clone()));
    let update_resource_usage_usecase =
        Arc::new(UpdateResourceUsageUseCase::new(resource_usage_repo.clone()));
    let delete_resource_usage_usecase =
        Arc::new(DeleteResourceUsageUseCase::new(resource_usage_repo.clone()));

    // Tokenの読み込み
    let bot_token = SlackApiToken::new(app_config.slack_bot_token.clone().into());

    // SlackAppの作成
    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let app = Arc::new(SlackApp::new(
        grant_access_usecase,
        create_resource_usage_usecase,
        update_resource_usage_usecase,
        delete_resource_usage_usecase,
        identity_repo.clone(),
        config_arc.clone(),
        slack_client,
        bot_token,
    ));
    println!("✅ Slack App を初期化しました");

    // 通知機能のセットアップ
    let notifier = NotificationRouter::new(config_arc.as_ref().clone(), identity_repo.clone());

    // ポーリング用にも同じリポジトリインスタンスを使用（IdMapperを共有するため）
    let notify_usecase =
        NotifyFutureResourceUsageChangesUseCase::new(Arc::clone(&resource_usage_repo), notifier)
            .await
            .map_err(|e| format!("通知UseCaseの初期化に失敗: {}", e))?;

    let notify_usecase = Arc::new(notify_usecase);
    println!("✅ 通知機能を初期化しました");

    // Socket Modeのセットアップ
    let slack_app_token = app_config.slack_app_token.clone();

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

        // Appを状態から取得
        let app = state
            .read()
            .await
            .get_user_state::<Arc<SlackApp<GoogleCalendarUsageRepository>>>()
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

    // インタラクションハンドラ関数
    async fn handle_interaction_event(
        event: SlackInteractionEvent,
        client: Arc<SlackHyperClient>,
        state: SlackClientEventsUserState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("🔘 インタラクションを受信しました");

        let app = state
            .read()
            .await
            .get_user_state::<Arc<SlackApp<GoogleCalendarUsageRepository>>>()
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
                            // Get the view ID from the event
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
                            // Get trigger_id from event
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
                            // Not implemented for now
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

        // Socket Modeには即座に応答を返す
        Ok(())
    }

    let socket_mode_callbacks = SlackSocketModeListenerCallbacks::new()
        .with_command_events(handle_command_event)
        .with_interaction_events(handle_interaction_event);

    let slack_client_for_env = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let listener_environment = Arc::new(
        SlackClientEventsListenerEnvironment::new(slack_client_for_env)
            .with_user_state(app.clone()),
    );

    let socket_mode_listener = SlackClientSocketModeListener::new(
        &SlackClientSocketModeConfig::new(),
        listener_environment.clone(),
        socket_mode_callbacks,
    );

    println!("🔌 Slack Socket Mode に接続しています...");

    socket_mode_listener
        .listen_for(&SlackApiToken::new(slack_app_token.into()))
        .await?;

    println!("✅ Slack Socket Mode に接続しました！");
    println!("🎉 Bot がスラッシュコマンドを待機しています");
    println!();

    println!(
        "🔍 カレンダー監視を開始します（間隔: {}秒）",
        app_config.polling_interval_secs
    );
    println!();
    println!("Bot を停止するには Ctrl+C を押してください");

    // バックグラウンドでポーリングタスクを実行
    let polling_handle = {
        let notify_usecase = notify_usecase.clone();
        let polling_interval = Duration::from_secs(app_config.polling_interval_secs);
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

    Ok(())
}
