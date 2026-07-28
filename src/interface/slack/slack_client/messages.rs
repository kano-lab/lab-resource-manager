//! Slack message operations
//!
//! Wrappers around Slack API for message operations

use slack_morphism::prelude::*;
use tracing::{debug, error};

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
        Ok(_) => debug!("follow-up message sent"),
        Err(e) => error!(error = %e, "sending the follow-up message failed"),
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
        Ok(_) => debug!("ephemeral message sent"),
        Err(e) => error!(error = %e, "sending the ephemeral message failed"),
    }
}
