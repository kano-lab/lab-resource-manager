//! バックグラウンドタスク実行
//!
//! スラッシュコマンドにすぐ応答し、時間のかかる処理の結果を後から届けるユーティリティ

use crate::interface::slack::slack_client::messages;
use slack_morphism::prelude::*;
use tokio_util::task::TaskTracker;

/// コマンドには空の応答を返し、処理の結果を後からresponse URL経由で届ける
///
/// 結果はエフェメラルメッセージ（コマンド実行ユーザーのみ表示）として送信されます。
///
/// 「処理中」のような仮のメッセージは返さない。結果は新しいメッセージとして届くため、
/// 仮のメッセージが結果の上に残り続けてしまう。
///
/// # 引数
/// * `task_tracker` - 終了時に処理の完了を待つためのTaskTracker
/// * `http_client` - 結果の送信に使うHTTPクライアント
/// * `response_url` - 結果を届けるresponse URL
/// * `operation` - 届ける内容を組み立てる処理。失敗時の内容も呼び出し側が組み立てる
///
/// # 戻り値
/// Slackへすぐに返す空の応答
pub fn respond_later<Fut>(
    task_tracker: &TaskTracker,
    http_client: reqwest::Client,
    response_url: SlackResponseUrl,
    operation: Fut,
) -> SlackCommandEventResponse
where
    Fut: std::future::Future<Output = SlackMessageContent> + Send + 'static,
{
    task_tracker.spawn(async move {
        let content = operation.await;
        messages::post_ephemeral(&http_client, &response_url, content).await;
    });

    SlackCommandEventResponse::new(SlackMessageContent::new())
}
