//! /link-user コマンドハンドラ

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /link-user スラッシュコマンドを処理
///
/// ユーザーリンクモーダルを開く（管理者コマンド）
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    info!("🔗 ユーザーリンクモーダルを開きます");

    // ユーザーリンクモーダルを作成
    let modal = views::modals::link_user::create(app.resource_config(), None);

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
