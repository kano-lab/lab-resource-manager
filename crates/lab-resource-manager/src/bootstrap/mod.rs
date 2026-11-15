//! コンポジションルート
//!
//! 設定から依存を組み立て、プロセスの始まりから終わりまでを受け持つ。
//!
//! - 定期ジョブ（予約の変化の通知、実利用の観測）は、どの入口が動いているかに関わらず続ける
//! - 入口（Slackボット、MCPサーバー）は、設定されているものだけを始める。
//!   Slackがなければ、予約者へDMで知らせる実利用の観測も始めない
//! - 終了の合図を受けたら、すべてに止まるよう伝え、止まるのを待つ

mod jobs;
mod mcp;
mod services;
mod slack;

use crate::infrastructure::config::{AppConfig, ResourceConfig, StorageConfig};
use services::Services;
use slack::SlackConnection;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// 起動から終了までに起きるエラー
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// 依存を組み立てて動かし、終了の合図かSlackとの接続の終わりまで続ける
///
/// # Errors
/// 依存を組み立てられない場合（Google Calendarにつなげない、TLS証明書を読めないなど）や、
/// Slackとの接続を始められない場合
pub async fn run(
    app_config: AppConfig,
    resource_config: ResourceConfig,
    storage_config: StorageConfig,
) -> Result<(), BoxError> {
    let services = Services::build(&app_config, resource_config, storage_config).await?;
    let slack = app_config
        .slack
        .as_ref()
        .map(SlackConnection::new)
        .transpose()?;
    let shutdown = CancellationToken::new();

    let notifications =
        jobs::start_reservation_notifications(&app_config, &services, shutdown.clone()).await?;
    let observation =
        jobs::start_slot_observation(&app_config, &services, slack.as_ref(), shutdown.clone());
    let mcp = mcp::start(&app_config, &services, shutdown.clone()).await?;
    if slack.is_none() && mcp.is_none() {
        warn!("no entry point is configured: only reservation change notifications run");
    }
    let tasks: Vec<JoinHandle<()>> = std::iter::once(notifications)
        .chain(observation)
        .chain(mcp)
        .collect();

    tokio::spawn(cancel_on_signal(shutdown.clone()));
    let result = serve_slack(&services, slack, shutdown.clone()).await;

    info!("shutting down");
    shutdown.cancel();
    for task in tasks {
        // 止めたタスクの終わり方はそれぞれがログに残している
        let _ = task.await;
    }

    result
}

/// Slackボットを動かす（Slackが設定されていなければ、終了の合図まで待つ）
async fn serve_slack(
    services: &Services,
    slack: Option<SlackConnection>,
    shutdown: CancellationToken,
) -> Result<(), BoxError> {
    match slack {
        Some(slack) => slack::build_bot(services, slack).run(shutdown).await,
        None => {
            info!("slack bot disabled: SLACK_BOT_TOKEN and SLACK_APP_TOKEN are not set");
            shutdown.cancelled().await;
            Ok(())
        }
    }
}

/// 終了の合図（Ctrl+C）を受けたら、すべてに止まるよう伝える
async fn cancel_on_signal(shutdown: CancellationToken) {
    if tokio::signal::ctrl_c().await.is_ok() {
        info!("received shutdown signal");
    }
    shutdown.cancel();
}
