//! /reserve コマンドハンドラ

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /reserve スラッシュコマンドを処理
///
/// リソース予約モーダルを開く
pub async fn handle<R: ResourceUsageRepository>(
    app: &SlackApp<R>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let user_id = event.user_id.to_string();
    info!("📅 リソース予約モーダルを開きます: user={}", user_id);

    // user_id と channel_id のマッピングを保存
    app.user_channel_map
        .write()
        .unwrap()
        .insert(event.user_id.clone(), event.channel_id.clone());

    // リソース予約モーダルを作成
    let modal = views::modals::reserve::create();

    // モーダルを開く
    modals::open(&app.slack_client, &app.bot_token, &event.trigger_id, modal).await?;

    // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
