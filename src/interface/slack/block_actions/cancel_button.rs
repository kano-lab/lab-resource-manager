//! 予約キャンセルボタンハンドラ

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::{interaction_reply, reservation_failure, user_resolver};
use slack_morphism::prelude::*;
use tracing::{debug, error, info};

/// 予約キャンセルボタンのクリックを処理
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
        error!("cancel button carried no usage id");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("interaction carried no user information");
        return Ok(());
    };

    debug!(usage_id = %usage_id_str, "cancel requested");

    // エフェメラルメッセージの宛先として、このユーザーが最後にいたチャンネルを控える
    let channel_id = interaction_reply::channel_id(block_actions);
    if let Some(channel_id) = &channel_id {
        app.user_channel_map()
            .write()
            .unwrap()
            .insert(user.id.clone(), channel_id.clone());
    }

    // 依存性を取得
    let delete_usage_usecase = app.delete_usage_usecase();
    let identity_repo = app.identity_repo();

    // ユーザーのメールアドレスを取得
    let owner_email = user_resolver::resolve_user_email(&user.id, identity_repo).await?;

    // 予約を削除
    let usage_id = UsageId::from_string(usage_id_str.to_string());
    info!(
        usage_id = %usage_id.as_str(),
        owner = %owner_email.as_str(),
        origin = "slack",
        "cancelling a reservation"
    );

    let result = delete_usage_usecase
        .execute(&usage_id, &EmailAddress::new(owner_email.clone())?)
        .await;

    // ユーザーにフィードバックメッセージを送信
    if let Some(ch_id) = channel_id {
        let message_text = match &result {
            Ok(_) => {
                info!(usage_id = %usage_id.as_str(), origin = "slack", "reservation cancelled");
                "✅ 予約をキャンセルしました".to_string()
            }
            Err(e) => {
                error!(usage_id = %usage_id.as_str(), origin = "slack", error = %e, "cancelling the reservation failed");
                reservation_failure::cancel_message(e)
            }
        };

        // エフェメラルメッセージで結果を通知
        let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
            ch_id,
            user.id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );

        let session = app.slack_client().open_session(app.bot_token());
        if let Err(e) = session.chat_post_ephemeral(&ephemeral_req).await {
            error!(error = %e, "sending the ephemeral message failed");
        }
    } else {
        error!("cannot send the ephemeral message: no channel id");
    }

    // エラーの場合もOkを返す（ユーザーには既にメッセージを送信済み）
    // これにより、Slackに「エラーが発生しました」というデフォルトメッセージが表示されない
    Ok(())
}
