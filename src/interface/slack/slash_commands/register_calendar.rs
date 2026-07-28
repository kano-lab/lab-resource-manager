//! /register-calendar コマンドハンドラ（非推奨）

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::debug;

/// /register-calendar スラッシュコマンドを処理（非推奨）
///
/// メールアドレス登録モーダルを開く
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let user_id = event.user_id.to_string();
    debug!(user = %user_id, "opening the email registration modal");

    // メールアドレス登録モーダルを作成
    let modal = views::modals::registration::create();

    // モーダルを開く
    modals::open(
        app.slack_client(),
        app.bot_token(),
        &event.trigger_id,
        modal,
    )
    .await?;

    // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
