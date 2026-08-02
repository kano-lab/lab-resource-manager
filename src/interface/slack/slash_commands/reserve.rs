//! /reserve コマンドハンドラ

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::modals::{registration, reserve};
use slack_morphism::prelude::*;
use tracing::{debug, info};

/// /reserve スラッシュコマンドを処理
///
/// ユーザーが紐付け済みの場合は予約モーダルを表示、未紐付けの場合はメール登録モーダルを表示
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let user_id = &event.user_id;
    let trigger_id = &event.trigger_id;

    // Get dependencies
    let config = app.resource_config();
    let slack_client = app.slack_client();
    let bot_token = app.bot_token();
    let identity_repo = &app.repositories().identity_link;

    // Check if user is linked
    let is_linked = user_resolver::is_user_linked(user_id, identity_repo).await;

    if !is_linked {
        // Unlinked: Show email registration modal
        info!(
            slack_user = %user_id,
            "the user has no identity link; opening the email registration modal"
        );

        let modal = registration::create();
        modals::open(slack_client, bot_token, trigger_id, modal).await?;

        debug!("email registration modal opened");
        return Ok(SlackCommandEventResponse::new(SlackMessageContent::new()));
    }

    // Linked: Show reservation modal
    info!(
        slack_user = %user_id,
        "opening the reservation modal"
    );

    // Create and open reservation modal
    let initial_server = config.servers.first().map(|s| s.name.as_str());
    let modal = reserve::create_reserve_modal(config, None, initial_server, None, None, None, None);

    modals::open(slack_client, bot_token, trigger_id, modal).await?;

    debug!("reservation modal opened");
    Ok(SlackCommandEventResponse::new(SlackMessageContent::new()))
}
