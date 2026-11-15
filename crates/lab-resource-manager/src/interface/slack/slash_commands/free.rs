//! /free コマンドハンドラ

use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotQuery};
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::async_execution::background_task;
use crate::interface::slack::utility::availability_report;
use slack_morphism::prelude::*;
use std::sync::Arc;
use tracing::{debug, error};

/// /free スラッシュコマンドを処理
///
/// いま空いているリソースを、本人にのみ見えるエフェメラルメッセージで返す。
/// 応答に置いたボタンから、この先の日の空きへ切り替えられる。
///
/// 空きを調べるには全リソースのカレンダーを読む必要があり、Slackがコマンドに求める
/// 3秒の応答期限を超えることがある。そのためコマンドにはすぐ応答し、結果は後から届ける。
pub async fn handle<R>(
    app: &Arc<SlackApp<R>>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ReservationRepository + ReservedSlotQuery + Send + Sync + 'static,
{
    debug!(slack_user = %event.user_id, "checking resource availability");

    let report_app = Arc::clone(app);
    let report = async move {
        availability_report::now_view(&report_app)
            .await
            .unwrap_or_else(|e| {
                error!(command = "/free", error = %e, "slash command failed");
                SlackMessageContent::new().with_text(format!("エラー: {}", e))
            })
    };

    Ok(background_task::respond_later(
        app.task_tracker(),
        app.http_client().clone(),
        event.response_url,
        report,
    ))
}
