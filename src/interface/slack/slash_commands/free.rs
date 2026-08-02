//! /free コマンドハンドラ

use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::availability_report;
use slack_morphism::prelude::*;
use tracing::debug;

/// /free スラッシュコマンドを処理
///
/// いま空いているリソースを、本人にのみ見えるエフェメラルメッセージで返す。
/// 応答に置いたボタンから、この先の日の空きへ切り替えられる。
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    debug!(slack_user = %event.user_id, "checking resource availability");

    let content = availability_report::now_view(app).await?;

    Ok(SlackCommandEventResponse::new(content))
}
