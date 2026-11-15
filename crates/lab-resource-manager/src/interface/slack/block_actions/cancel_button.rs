//! 予約キャンセルボタンハンドラ

use crate::domain::aggregates::reservation::value_objects::ReservedSlotId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotQuery};
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
    R: ReservationRepository + ReservedSlotQuery + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let Some(slot_id_str) = &action.value else {
        error!("cancel button carried no reserved slot id");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("interaction carried no user information");
        return Ok(());
    };

    debug!(reserved_slot_id = %slot_id_str, "cancel requested");

    // エフェメラルメッセージの宛先として、このユーザーが最後にいたチャンネルを控える
    let channel_id = interaction_reply::channel_id(block_actions);
    if let Some(channel_id) = &channel_id {
        app.user_channel_map()
            .write()
            .unwrap()
            .insert(user.id.clone(), channel_id.clone());
    }

    // 依存性を取得
    let cancel_reserved_slot_usecase = &app.usecases().cancel_reserved_slot;
    let identity_repo = &app.repositories().identity_link;

    // ユーザーのメールアドレスを取得
    let holder_email = user_resolver::resolve_user_email(&user.id, identity_repo).await?;

    // 予約を削除
    let slot_id = ReservedSlotId::from_string(slot_id_str.to_string());
    info!(
        reserved_slot_id = %slot_id.as_str(),
        holder = %holder_email.as_str(),
        origin = "slack",
        "cancelling a reservation"
    );

    let result = cancel_reserved_slot_usecase
        .execute(&slot_id, &EmailAddress::new(holder_email.clone())?)
        .await;

    // ユーザーにフィードバックメッセージを送信
    if let Some(ch_id) = channel_id {
        let message_text = match &result {
            Ok(event) => {
                info!(reserved_slot_id = %slot_id.as_str(), origin = "slack", "reserved slot cancelled");
                reservation_failure::cancelled_message(event)
            }
            Err(e) => {
                error!(reserved_slot_id = %slot_id.as_str(), origin = "slack", error = %e, "cancelling the reservation failed");
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
