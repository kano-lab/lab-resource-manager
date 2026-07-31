//! 予約を今の時点で終了するボタンハンドラ

use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::{
    interaction_reply, reservation_failure, reservation_summary, user_resolver,
};
use slack_morphism::prelude::*;
use tracing::{error, info, warn};

/// 「今で終了」ボタンのクリックを処理
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let Some(usage_id_str) = &action.value else {
        error!("release button carried no usage id");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("interaction carried no user information");
        return Ok(());
    };

    let Some(channel_id) = interaction_reply::channel_id(block_actions) else {
        error!("cannot send the ephemeral message: no channel id");
        return Ok(());
    };

    let requested_by =
        EmailAddress::new(user_resolver::resolve_user_email(&user.id, app.identity_repo()).await?)?;
    let usage_id = UsageId::from_string(usage_id_str.to_string());

    info!(
        usage_id = %usage_id.as_str(),
        requested_by = %requested_by.as_str(),
        origin = "slack",
        "releasing a reservation early"
    );

    let outcome = app
        .release_early_usecase()
        .execute(&usage_id, &requested_by)
        .await;

    let feedback = match &outcome {
        Ok(released) => {
            info!(
                usage_id = %usage_id.as_str(),
                end = %released.time_period().end(),
                origin = "slack",
                "reservation released early"
            );
            format!(
                "✅ 予約を終了しました。残りの時間を解放しました。\n\n{}",
                reservation_summary::build(released, app.resource_config())
            )
        }
        Err(failure) => {
            log_failure(&usage_id, failure);
            reservation_failure::release_message(failure)
        }
    };

    let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
        channel_id,
        user.id.clone(),
        SlackMessageContent::new().with_text(feedback),
    );

    let session = app.slack_client().open_session(app.bot_token());
    if let Err(e) = session.chat_post_ephemeral(&ephemeral_req).await {
        error!(error = %e, "sending the ephemeral message failed");
    }

    // 利用者には結果を伝え済みのため、Slackの既定のエラー表示は出さない
    Ok(())
}

/// 終了できなかった事実を記録する
///
/// まだ始まっていない・すでに終わった・他人の予約といった結果は、ボタンを押した
/// タイミングから生まれる正常な結果であり、運用者に対処できることはない。
/// `error`に置くと押されるたびに運用者を呼ぶことになるためwarnとする。
fn log_failure(usage_id: &UsageId, failure: &ApplicationError) {
    match failure {
        ApplicationError::ResourceUsage(_) | ApplicationError::Unauthorized(_) => warn!(
            usage_id = %usage_id.as_str(),
            reason = %failure,
            origin = "slack",
            "releasing the reservation early was refused"
        ),
        other => error!(
            usage_id = %usage_id.as_str(),
            error = %other,
            origin = "slack",
            "releasing the reservation early failed"
        ),
    }
}
