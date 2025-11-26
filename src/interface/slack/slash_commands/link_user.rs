//! /link-user コマンドハンドラ

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /link-user スラッシュコマンドを処理
///
/// ユーザーリンクモーダルを開く（管理者コマンド）
pub async fn handle<R: ResourceUsageRepository>(
    app: &SlackApp<R>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    info!("🔗 ユーザーリンクモーダルを開きます");

    // user_id と channel_id のマッピングを保存
    app.user_channel_map
        .write()
        .unwrap()
        .insert(event.user_id.clone(), event.channel_id.clone());

    // ユーザーリンクモーダルを作成
    let modal = views::modals::link_user::create();

    // モーダルを開く
    modals::open(&app.slack_client, &app.bot_token, &event.trigger_id, modal).await?;

    // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
