//! 未使用の予約を知らせるDMのボタンハンドラ
//!
//! 予約者に示す3つの答え（今で終了する・まだ使う・予約を取り消す）を扱う。
//! いずれも押したあとはメッセージを結果で置き換え、ボタンを残さない。

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{
    ACTION_IDLE_CANCEL, ACTION_IDLE_KEEP, ACTION_IDLE_RELEASE,
};
use crate::interface::slack::utility::{
    interaction_reply, reservation_failure, reservation_summary, user_resolver,
};
use chrono::Utc;
use slack_morphism::prelude::*;
use tracing::{error, info, warn};

/// 未使用予約のお知らせDMのボタンのクリックを処理
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
        error!("idle reservation button carried no usage id");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("interaction carried no user information");
        return Ok(());
    };

    let usage_id = UsageId::from_string(usage_id_str.to_string());
    let action_id = action.action_id.to_string();

    let feedback = match action_id.as_str() {
        ACTION_IDLE_KEEP => keep(app, &usage_id),
        ACTION_IDLE_RELEASE | ACTION_IDLE_CANCEL => {
            let requested_by = EmailAddress::new(
                user_resolver::resolve_user_email(&user.id, &app.repositories().identity_link)
                    .await?,
            )?;

            if action_id == ACTION_IDLE_RELEASE {
                release(app, &usage_id, &requested_by).await
            } else {
                cancel(app, &usage_id, &requested_by).await
            }
        }
        other => {
            error!(action_id = %other, "unknown idle reservation action");
            return Ok(());
        }
    };

    reply(app, block_actions, feedback).await;
    Ok(())
}

/// 「まだ使う」に答える
///
/// 予約は使われる見込みなので、しばらくこの予約について声をかけない。
fn keep<R, N>(app: &SlackApp<R, N>, usage_id: &UsageId) -> String
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    app.idle_notices().silence(usage_id, Utc::now());
    info!(
        usage_id = %usage_id.as_str(),
        origin = "slack",
        "the owner intends to keep using the reservation"
    );
    "👍 わかりました。しばらくこの予約については知らせません。".to_string()
}

/// 「今で終了する」に答える
async fn release<R, N>(
    app: &SlackApp<R, N>,
    usage_id: &UsageId,
    requested_by: &EmailAddress,
) -> String
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    info!(
        usage_id = %usage_id.as_str(),
        requested_by = %requested_by.as_str(),
        origin = "slack",
        "releasing an idle reservation early"
    );

    match app
        .usecases()
        .release_resource_usage_early
        .execute(usage_id, requested_by)
        .await
    {
        Ok(released) => format!(
            "✅ 予約を終了しました。残りの時間を解放しました。\n\n{}",
            reservation_summary::build(&released, app.resource_config())
        ),
        Err(failure) => {
            warn!(
                usage_id = %usage_id.as_str(),
                reason = %failure,
                origin = "slack",
                "releasing the idle reservation was refused"
            );
            reservation_failure::release_message(&failure)
        }
    }
}

/// 「予約を取り消す」に答える
async fn cancel<R, N>(
    app: &SlackApp<R, N>,
    usage_id: &UsageId,
    requested_by: &EmailAddress,
) -> String
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    info!(
        usage_id = %usage_id.as_str(),
        requested_by = %requested_by.as_str(),
        origin = "slack",
        "cancelling an idle reservation"
    );

    match app
        .usecases()
        .delete_resource_usage
        .execute(usage_id, requested_by)
        .await
    {
        Ok(()) => "✅ 予約を取り消しました。".to_string(),
        Err(failure) => {
            warn!(
                usage_id = %usage_id.as_str(),
                reason = %failure,
                origin = "slack",
                "cancelling the idle reservation was refused"
            );
            reservation_failure::cancel_message(&failure)
        }
    }
}

/// 押されたボタンごと、DMを操作の結果で置き換える
///
/// 置き換えられなかった場合に限り、結果を新しいメッセージとして送る。
/// ボタンは残ってしまうが、何が起きたのかは必ず伝わるようにする。
async fn reply<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
    feedback: String,
) where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let session = app.slack_client().open_session(app.bot_token());

    if let Some((channel_id, message_ts)) = interaction_reply::message_ref(block_actions) {
        let settled = SlackApiChatUpdateRequest::new(
            channel_id.clone(),
            interaction_reply::settled_message(feedback.clone()),
            message_ts,
        );

        match session.chat_update(&settled).await {
            Ok(updated) => {
                info!(channel = %updated.channel, ts = %updated.ts, "settled the idle reservation dm");
                return;
            }
            Err(e) => warn!(error = %e, "replacing the idle reservation dm failed"),
        }
    }

    let Some(channel_id) = interaction_reply::channel_id(block_actions) else {
        error!("cannot tell the outcome: no channel id");
        return;
    };

    let post_req = SlackApiChatPostMessageRequest::new(
        channel_id,
        SlackMessageContent::new().with_text(feedback),
    );
    if let Err(e) = session.chat_post_message(&post_req).await {
        error!(error = %e, "sending the outcome message failed");
    }
}
