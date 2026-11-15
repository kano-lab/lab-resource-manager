//! Slack modal operations
//!
//! Wrappers around Slack API for modal operations

use slack_morphism::prelude::*;
use std::sync::Arc;
use tracing::{debug, error};

/// モーダルビューを開く
///
/// # 引数
/// * `client` - Slack HTTPクライアント
/// * `token` - Botトークン
/// * `trigger_id` - インタラクションイベントからのトリガーID
/// * `view` - 開くモーダルビュー
pub async fn open(
    client: &Arc<SlackHyperClient>,
    token: &SlackApiToken,
    trigger_id: &SlackTriggerId,
    view: SlackView,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("opening a modal");

    let session = client.open_session(token);
    let request = SlackApiViewsOpenRequest::new(trigger_id.clone(), view);

    match session.views_open(&request).await {
        Ok(_) => {
            debug!("modal opened");
            Ok(())
        }
        Err(e) => {
            error!(error = ?e, "opening the modal failed");
            Err(e.into())
        }
    }
}

/// 既存のモーダルビューを更新
///
/// # 引数
/// * `client` - Slack HTTPクライアント
/// * `token` - Botトークン
/// * `view_id` - 更新するビューのID
/// * `view` - 新しいモーダルビュー
pub async fn update(
    client: &Arc<SlackHyperClient>,
    token: &SlackApiToken,
    view_id: &SlackViewId,
    view: SlackView,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!(view_id = %view_id, "updating a modal");

    let session = client.open_session(token);
    let request = SlackApiViewsUpdateRequest::new(view).with_view_id(view_id.clone());

    match session.views_update(&request).await {
        Ok(_) => {
            debug!("modal updated");
            Ok(())
        }
        Err(e) => {
            error!(error = ?e, "updating the modal failed");
            Err(e.into())
        }
    }
}
