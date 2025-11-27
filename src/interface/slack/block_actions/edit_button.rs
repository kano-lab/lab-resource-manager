//! 予約編集ボタンハンドラ

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::CALLBACK_RESERVE_UPDATE;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::modals::{registration, reserve};
use slack_morphism::prelude::*;
use tracing::error;

/// 予約編集ボタンのクリックを処理
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(usage_id_str) = &action.value else {
        error!("❌ usage_idが取得できませんでした");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("❌ ユーザー情報が取得できませんでした");
        return Ok(());
    };

    println!("🔄 予約更新要求: usage_id={}", usage_id_str);

    // 依存性を取得
    let slack_client = &app.slack_client;
    let bot_token = &app.bot_token;
    let identity_repo = &app.identity_repo;
    let config = &app.resource_config;

    let trigger_id = &block_actions.trigger_id;

    // ユーザーがリンクされているかチェック
    let is_linked = user_resolver::is_user_linked(&user.id, identity_repo).await;

    if !is_linked {
        // 未リンク: メールアドレス登録モーダルを表示
        println!(
            "ユーザー {} は未リンク。メールアドレス登録モーダルを表示します",
            user.id
        );

        let modal = registration::create();
        modals::open(slack_client, bot_token, trigger_id, modal).await?;

        return Ok(());
    }

    // リンク済み: 更新モーダルを開く（usage_idをprivate_metadataに設定）
    println!("予約更新モーダルを開きます（予約ID: {}）", usage_id_str);

    // 予約モーダルを作成（更新用のパラメータを渡す）
    let initial_server = config.servers.first().map(|s| s.name.as_str());
    let modal_view = reserve::create_reserve_modal(
        config,
        None,
        initial_server,
        Some(usage_id_str),
        Some(CALLBACK_RESERVE_UPDATE),  // callback_id
        Some("予約更新"),                // title
        Some("更新"),                    // submit_text
    );

    println!("  → 更新モーダルを作成: callback_id={}", CALLBACK_RESERVE_UPDATE);

    modals::open(slack_client, bot_token, trigger_id, modal_view).await?;

    println!("✅ 更新モーダルを開きました（予約ID: {}）", usage_id_str);
    Ok(())
}
