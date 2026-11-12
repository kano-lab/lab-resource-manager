//! リソース予約更新モーダル送信ハンドラ

use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::reservation::value_objects::ReservedSlotId;
use crate::domain::common::TimePeriod;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotRepository};
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::utility::conflict_message;
use crate::interface::slack::utility::{datetime_parser::parse_datetime, extract_form_data};
use slack_morphism::prelude::*;

/// リソース予約更新モーダル送信を処理
///
/// 既存の予約を更新し、エフェメラルメッセージで結果を通知
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ReservationRepository + ReservedSlotRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let user_id = view_submission.user.id.clone();

    // private_metadataからslot_idを取得
    let slot_id_str = extract_form_data::get_private_metadata(view_submission)
        .ok_or("slot_idがprivate_metadataに設定されていません")?;

    let slot_id = ReservedSlotId::from_string(slot_id_str.clone());

    // 開始・終了日時を取得
    let start_date =
        extract_form_data::get_selected_date(view_submission, ACTION_RESERVE_START_DATE)
            .ok_or("開始日が選択されていません")?;
    let start_time =
        extract_form_data::get_selected_time(view_submission, ACTION_RESERVE_START_TIME)
            .ok_or("開始時刻が選択されていません")?;

    let end_date = extract_form_data::get_selected_date(view_submission, ACTION_RESERVE_END_DATE)
        .ok_or("終了日が選択されていません")?;
    let end_time = extract_form_data::get_selected_time(view_submission, ACTION_RESERVE_END_TIME)
        .ok_or("終了時刻が選択されていません")?;

    // Parse datetime
    let start_datetime = parse_datetime(&start_date, &start_time)?;
    let end_datetime = parse_datetime(&end_date, &end_time)?;
    let time_period = TimePeriod::new(start_datetime, end_datetime)?;

    // 備考を取得（オプション）
    let notes = extract_form_data::get_plain_text_input(view_submission, ACTION_RESERVE_NOTES);

    // ユーザーのメールアドレスを取得
    let identity_link = app
        .repositories()
        .identity_link
        .find_by_external_user_id(&ExternalSystem::Slack, user_id.as_ref())
        .await?
        .ok_or("ユーザーが登録されていません。まず /register-calendar を実行してください")?;

    let holder_email = identity_link.email().clone();

    // 予約を更新
    let update_result = app
        .usecases()
        .update_slot
        .execute(&slot_id, &holder_email, Some(time_period), notes)
        .await;

    // channel_id を取得
    let channel_id = app
        .user_channel_map()
        .read()
        .unwrap()
        .get(&user_id)
        .cloned()
        .ok_or("セッションの有効期限が切れました。もう一度コマンドを実行してください。")?;

    // エフェメラルメッセージで結果を送信
    let message_text = match update_result {
        Ok(_) => "✅ 予約を更新しました".to_string(),
        Err(ApplicationError::ResourceConflict { conflicts }) => {
            let conflict_detail = conflict_message::build(
                &conflicts,
                app.resource_config(),
                &app.repositories().identity_link,
            )
            .await;
            format!("❌ 予約の更新に失敗しました\n\n{}", conflict_detail)
        }
        Err(e) => {
            // エラーの種類に応じてユーザーフレンドリーなメッセージを返す
            let error_msg = e.to_string();
            if error_msg.contains("見つかりません") || error_msg.contains("NotFound") {
                "❌ 申し訳ございません。この予約は既に削除されているか、見つかりませんでした。"
                    .to_string()
            } else if error_msg.contains("権限") || error_msg.contains("Unauthorized") {
                "❌ この予約を更新する権限がありません。".to_string()
            } else {
                format!("❌ 予約の更新に失敗しました: {}", error_msg)
            }
        }
    };

    let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
        channel_id,
        user_id.clone(),
        SlackMessageContent::new().with_text(message_text),
    );

    let session = app.slack_client().open_session(app.bot_token());
    session.chat_post_ephemeral(&ephemeral_req).await?;

    // モーダルを閉じる
    Ok(None)
}
