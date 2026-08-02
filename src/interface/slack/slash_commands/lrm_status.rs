//! /lrm-status コマンドハンドラ

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::views::messages::monitoring;
use chrono::Utc;
use slack_morphism::prelude::*;
use tracing::{debug, info};

/// /lrm-status スラッシュコマンドを処理
///
/// 実利用の監視がいま効いているかを、本人にのみ見えるメッセージで返す。
/// 監視が止まっていても突合が黙るだけなので、確かめる手段がないと気づけない。
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    debug!(user = %event.user_id, "reporting the monitoring status");

    let status = app.usecases().describe_monitoring.execute().await?;
    let text = monitoring::build(
        &status,
        env!("CARGO_PKG_VERSION"),
        app.started_at(),
        Utc::now(),
    );

    info!(
        slack_user = %event.user_id,
        version = env!("CARGO_PKG_VERSION"),
        "reported the monitoring status"
    );

    Ok(SlackCommandEventResponse::new(
        SlackMessageContent::new().with_text(text),
    ))
}
