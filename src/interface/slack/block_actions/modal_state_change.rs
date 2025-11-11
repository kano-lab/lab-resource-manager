//! モーダル状態変更ハンドラ（リソースタイプ、サーバー選択）

use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views::modals::reservation;
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

    // Check dependencies
    let config = app
        .resource_config
        .as_ref()
        .ok_or("ResourceConfigが設定されていません")?;

    let slack_client = app
        .slack_client
        .as_ref()
        .ok_or("Slackクライアントが設定されていません")?;

    let bot_token = app
        .bot_token
        .as_ref()
        .ok_or("Bot tokenが設定されていません")?;

    // Determine new values based on action
    let new_resource_type = if action_id == ACTION_RESERVE_RESOURCE_TYPE {
        action.selected_option.as_ref().and_then(|opt| match &opt.text {
            SlackBlockText::Plain(plain) => {
                let text_val = plain.text.as_str();
                if text_val == "GPU Server" {
                    Some("gpu")
                } else if text_val == "Room" {
                    Some("room")
                } else {
                    None
                }
            }
            _ => None,
        })
    } else {
        None
    };

    let new_selected_server = if action_id == ACTION_RESERVE_SERVER_SELECT {
        action
            .selected_option
            .as_ref()
            .and_then(|opt| match &opt.text {
                SlackBlockText::Plain(plain) => Some(plain.text.as_str()),
                _ => None,
            })
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
    let updated_modal = reservation::create_reserve_modal(
        config,
        new_resource_type,
        new_selected_server,
        None, // No usage_id for modal updates
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
