//! モーダル状態変更ハンドラ（リソースタイプ、サーバー選択）

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views::modals::reserve;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// モーダル状態変更を処理（リソースタイプ選択、サーバー選択）
///
/// 適切なフィールドを表示するようモーダルを動的に更新
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let action_id = action.action_id.to_string();
    info!("🔄 モーダル更新トリガー検出: {}", action_id);

    // Get dependencies
    let config = &app.resource_config;
    let slack_client = &app.slack_client;
    let bot_token = &app.bot_token;

    // Determine new values based on action
    let new_resource_type = if action_id == ACTION_RESERVE_RESOURCE_TYPE {
        action
            .selected_option
            .as_ref()
            .map(|opt| opt.value.as_str())
    } else {
        None
    };

    // サーバー選択の決定
    let new_selected_server = if action_id == ACTION_RESERVE_SERVER_SELECT {
        // サーバーが明示的に選択された場合
        action
            .selected_option
            .as_ref()
            .map(|opt| opt.value.as_str())
    } else if new_resource_type.is_some() && new_resource_type == Some("gpu") {
        // リソースタイプがGPUに変更された場合、デフォルトのサーバーを選択
        config.servers.first().map(|s| s.name.as_str())
    } else {
        None
    };

    // Get view_id from container
    let view_id = match &block_actions.container {
        SlackInteractionActionContainer::View(view_container) => {
            info!(
                "  → view_id取得成功: {}",
                view_container.view_id.to_string()
            );
            view_container.view_id.clone()
        }
        SlackInteractionActionContainer::Message(_) => {
            error!("❌ モーダル外のインタラクションです");
            return Ok(());
        }
    };

    info!(
        "📝 選択値: type={:?}, server={:?}",
        new_resource_type, new_selected_server
    );

    // Create updated modal
    info!("🔨 新しいモーダルを作成中...");
    let updated_modal = reserve::create_reserve_modal(
        config,
        new_resource_type,
        new_selected_server,
        None, // No usage_id for modal updates
        None, // Use default callback_id
        None, // Use default title
        None, // Use default submit_text
    );

    // Update modal
    info!("🚀 Slack APIにモーダル更新をリクエスト中...");
    modals::update(slack_client, bot_token, &view_id, updated_modal).await?;

    info!(
        "✅ モーダルを動的に更新しました (view_id: {})",
        view_id.to_string()
    );

    Ok(())
}
