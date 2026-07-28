//! モーダル状態変更ハンドラ（リソースタイプ、サーバー選択）

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views::modals::{link_user, reserve};
use slack_morphism::prelude::*;
use tracing::{debug, error};

/// モーダル状態変更を処理（リソースタイプ選択、サーバー選択、リンク対象種別選択）
///
/// 適切なフィールドを表示するようモーダルを動的に更新
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let action_id = action.action_id.to_string();
    debug!(action_id = %action_id, "modal update triggered");

    // Get view_id from container
    let view_id = match &block_actions.container {
        SlackInteractionActionContainer::View(view_container) => {
            debug!(view_id = %view_container.view_id, "resolved the modal to update");
            view_container.view_id.clone()
        }
        SlackInteractionActionContainer::Message(_)
        | SlackInteractionActionContainer::MessageAttachment(_) => {
            error!("interaction did not come from a modal");
            return Ok(());
        }
    };

    let updated_modal = match action_id.as_str() {
        ACTION_RESERVE_RESOURCE_TYPE | ACTION_RESERVE_SERVER_SELECT => {
            build_updated_reserve_modal(app, &action_id, action)
        }
        ACTION_LINK_TARGET_TYPE => build_updated_link_user_modal(app, action),
        _ => return Ok(()),
    };

    // Update modal
    debug!("requesting a modal update");
    modals::update(app.slack_client(), app.bot_token(), &view_id, updated_modal).await?;

    debug!(view_id = %view_id, "modal updated");

    Ok(())
}

/// `/reserve`モーダルの再構築（リソースタイプ・サーバー選択変更）
fn build_updated_reserve_modal<R, N>(
    app: &SlackApp<R, N>,
    action_id: &str,
    action: &SlackInteractionActionInfo,
) -> SlackView
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let config = app.resource_config();

    let new_resource_type = if action_id == ACTION_RESERVE_RESOURCE_TYPE {
        action
            .selected_option
            .as_ref()
            .map(|opt| opt.value.as_str())
    } else {
        None
    };

    let new_selected_server = if action_id == ACTION_RESERVE_SERVER_SELECT {
        action
            .selected_option
            .as_ref()
            .map(|opt| opt.value.as_str())
    } else if new_resource_type == Some("gpu") {
        // リソースタイプがGPUに変更された場合、デフォルトのサーバーを選択
        config.servers.first().map(|s| s.name.as_str())
    } else {
        None
    };

    debug!(
        resource_type = ?new_resource_type,
        server = ?new_selected_server,
        "rebuilding the reservation modal from the current selection"
    );

    reserve::create_reserve_modal(
        config,
        new_resource_type,
        new_selected_server,
        None, // No usage_id for modal updates
        None, // Use default callback_id
        None, // Use default title
        None, // Use default submit_text
    )
}

/// `/link-user`モーダルの再構築（リンク対象種別変更）
fn build_updated_link_user_modal<R, N>(
    app: &SlackApp<R, N>,
    action: &SlackInteractionActionInfo,
) -> SlackView
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let new_target_type = action
        .selected_option
        .as_ref()
        .map(|opt| opt.value.as_str());

    debug!(target_type = ?new_target_type, "selection changed");

    link_user::create(app.resource_config(), new_target_type)
}
