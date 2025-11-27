//! /register-calendar コマンドハンドラ（非推奨）

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /register-calendar スラッシュコマンドを処理（非推奨）
///
/// メールアドレス登録モーダルを開く
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let user_id = event.user_id.to_string();
    info!("📧 メールアドレス登録モーダルを開きます: user={}", user_id);

    // メールアドレス登録モーダルを作成
    let modal = views::modals::registration::create();

    // モーダルを開く
    modals::open(&app.slack_client, &app.bot_token, &event.trigger_id, modal).await?;

    // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
