//! 予約編集ボタンハンドラ

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::adapters::user_resolver;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views::modals::{registration, reservation};
use slack_morphism::prelude::*;
use tracing::{error, info};

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

    info!("🔄 予約更新要求: usage_id={}", usage_id_str);

    // Get dependencies
    let config = &app.resource_config;
    let slack_client = &app.slack_client;
    let bot_token = &app.bot_token;
    let identity_repo = &app.identity_repo;

    let trigger_id = &block_actions.trigger_id;

    // Check if user is linked
    let is_linked = user_resolver::is_user_linked(&user.id, identity_repo).await;

    if !is_linked {
        // Unlinked: Show email registration modal
        info!(
            "ユーザー {} は未リンク。メールアドレス登録モーダルを表示します",
            user.id
        );

        let modal = registration::create();
        modals::open(slack_client, bot_token, trigger_id, modal).await?;

        return Ok(());
    }

    // Linked: Show update modal with usage_id in private_metadata
    // TODO: Pre-populate modal with existing reservation data
    info!("⚠️ 予約データの取得機能は未実装です。デフォルト値でモーダルを開きます。");

    let initial_server = config.servers.first().map(|s| s.name.as_str());
    let modal = reservation::create_reserve_modal(config, None, initial_server, Some(usage_id_str));

    modals::open(slack_client, bot_token, trigger_id, modal).await?;

    info!("✅ 更新モーダルを開きました（予約ID: {}）", usage_id_str);
    Ok(())
}
