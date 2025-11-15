//! Slack message operations
//!
//! Wrappers around Slack API for message operations

use slack_morphism::prelude::*;
use tracing::{error, info};

/// response URL経由でフォローアップメッセージを送信
///
/// # 引数
/// * `http_client` - HTTP client
/// * `response_url` - Slack response URL from the event
/// * `message` - Message text to send
pub async fn send_followup(
    http_client: &reqwest::Client,
    response_url: &SlackResponseUrl,
    message: String,
) {
    let payload = serde_json::json!({
        "text": message,
        "response_type": "in_channel"
    });

    match http_client
        .post(response_url.0.as_str())
        .json(&payload)
        .send()
        .await
    {
        Ok(_) => info!("✅ Follow-up message sent successfully"),
        Err(e) => error!("❌ Failed to send follow-up message: {}", e),
    }
}

/// エフェメラルメッセージを送信（ユーザーのみに表示）
///
/// # 引数
/// * `http_client` - HTTP client
/// * `response_url` - Slack response URL from the event
/// * `message` - Message text to send
pub async fn send_ephemeral(
    http_client: &reqwest::Client,
    response_url: &SlackResponseUrl,
    message: String,
) {
    let payload = serde_json::json!({
        "text": message,
        "response_type": "ephemeral"
    });

    match http_client
        .post(response_url.0.as_str())
        .json(&payload)
        .send()
        .await
    {
        Ok(_) => info!("✅ Ephemeral message sent successfully"),
        Err(e) => error!("❌ Failed to send ephemeral message: {}", e),
    }
}
