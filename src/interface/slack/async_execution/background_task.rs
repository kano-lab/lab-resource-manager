//! バックグラウンドタスク実行
//!
//! レスポンス追跡付きでバックグラウンドでタスクを実行するユーティリティ

use crate::interface::slack::slack_client::messages;
use slack_morphism::prelude::*;
use tokio_util::task::TaskTracker;

/// 操作をバックグラウンドで実行し、response URL経由で結果を送信
///
/// # 引数
/// * `task_tracker` - TaskTracker for managing background tasks
/// * `http_client` - HTTP client for sending follow-up messages
/// * `response_url` - Slack response URL to send the result to
/// * `operation` - Async operation to execute
///
/// # 戻り値
/// 処理開始を示す即時レスポンス
pub async fn execute_with_response<F, Fut>(
    task_tracker: &TaskTracker,
    http_client: reqwest::Client,
    response_url: SlackResponseUrl,
    operation: F,
) -> SlackCommandEventResponse
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    task_tracker.spawn(async move {
        let message = match operation().await {
            Ok(msg) => msg,
            Err(err) => err,
        };

        messages::send_followup(&http_client, &response_url, message).await;
    });

    SlackCommandEventResponse::new(SlackMessageContent::new().with_text("⏳ 処理中...".to_string()))
}
