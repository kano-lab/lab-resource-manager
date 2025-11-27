//! /reserve コマンドハンドラ

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::modals::{registration, reserve};
use slack_morphism::prelude::*;
use tracing::info;

/// /reserve スラッシュコマンドを処理
///
/// ユーザーが紐付け済みの場合は予約モーダルを表示、未紐付けの場合はメール登録モーダルを表示
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let user_id = &event.user_id;
    let trigger_id = &event.trigger_id;

    // Get dependencies
    let config = &app.resource_config;
    let slack_client = &app.slack_client;
    let bot_token = &app.bot_token;
    let identity_repo = &app.identity_repo;

    // Check if user is linked
    let is_linked = user_resolver::is_user_linked(user_id, identity_repo).await;

    if !is_linked {
        // Unlinked: Show email registration modal
        info!(
            "ユーザー {} は未リンク。メールアドレス登録モーダルを表示します",
            user_id
        );

        let modal = registration::create();
        modals::open(slack_client, bot_token, trigger_id, modal).await?;

        info!("✅ メールアドレス登録モーダルを開きました");
        return Ok(SlackCommandEventResponse::new(SlackMessageContent::new()));
    }

    // Linked: Show reservation modal
    info!(
        "ユーザー {} はリンク済み。予約モーダルを表示します",
        user_id
    );

    // Create and open reservation modal
    let initial_server = config.servers.first().map(|s| s.name.as_str());
    let modal = reserve::create_reserve_modal(config, None, initial_server, None);

    modals::open(slack_client, bot_token, trigger_id, modal).await?;

    info!("✅ 予約モーダルを開きました");
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
