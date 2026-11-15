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

/// エフェメラルメッセージを、いま表示されているものと差し替える
///
/// ボタンを押した結果を新しいメッセージとして積むと、古い状態が残ったまま
/// ボタンだけが増えていく。同じ場所を書き換えることで、画面は常に1つになる。
///
/// # 引数
/// * `http_client` - HTTP client
/// * `response_url` - Slack response URL from the event
/// * `content` - 差し替える内容
pub async fn replace_ephemeral(
    http_client: &reqwest::Client,
    response_url: &SlackResponseUrl,
    content: SlackMessageContent,
) {
    let payload = serde_json::json!({
        "text": content.text,
        "blocks": content.blocks,
        "response_type": "ephemeral",
        "replace_original": true
    });

    match http_client
        .post(response_url.0.as_str())
        .json(&payload)
        .send()
        .await
    {
        Ok(_) => debug!("ephemeral message replaced"),
        Err(e) => error!(error = %e, "replacing the ephemeral message failed"),
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
    post_ephemeral(
        http_client,
        response_url,
        SlackMessageContent::new().with_text(message),
    )
    .await;
}

/// ブロックを含む内容を、エフェメラルメッセージとして送信（ユーザーのみに表示）
///
/// response URLは期限切れなどでSlackに断られても通信自体は成功するため、
/// ステータスコードまで確かめて失敗を記録する。
///
/// # 引数
/// * `http_client` - HTTP client
/// * `response_url` - Slack response URL from the event
/// * `content` - 送信する内容
pub async fn post_ephemeral(
    http_client: &reqwest::Client,
    response_url: &SlackResponseUrl,
    content: SlackMessageContent,
) {
    // 空の項目を送らないよう、Slackの応答形式そのままで直列化する
    let payload = SlackCommandEventResponse::new(content)
        .with_response_type(SlackMessageResponseType::Ephemeral);

    let result = http_client
        .post(response_url.0.as_str())
        .json(&payload)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status);

    match result {
        Ok(_) => debug!("ephemeral message sent"),
        Err(e) => error!(error = %e, "sending the ephemeral message failed"),
    }
}
