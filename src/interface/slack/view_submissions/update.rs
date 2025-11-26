//! リソース予約更新モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::value_objects::{TimePeriod, UsageId};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{ACTION_END_TIME, ACTION_NOTES, ACTION_START_TIME};
use crate::interface::slack::utility::extract_form_data;
use chrono::{DateTime, Utc};
use slack_morphism::prelude::*;
use tracing::{error, info};

/// リソース予約更新モーダル送信を処理
///
/// 既存の予約を更新し、エフェメラルメッセージで結果を通知
pub async fn handle<R: ResourceUsageRepository>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("リソース予約更新を処理中...");

    let user_id = view_submission.user.id.clone();

    // private_metadataからusage_idを取得
    let usage_id_str = if let SlackView::Modal(modal) = &view_submission.view.view {
        modal
            .private_metadata
            .as_ref()
            .ok_or("usage_idがprivate_metadataに設定されていません")?
            .to_string()
    } else {
        return Err("モーダルビューが取得できません".into());
    };

    info!("予約ID: {}", usage_id_str);

    let usage_id = UsageId::from_string(usage_id_str.clone());

    // 開始・終了日時を取得
    let start_timestamp =
        extract_form_data::get_selected_datetime(view_submission, ACTION_START_TIME)
            .ok_or("開始日時が選択されていません")?;

    let end_timestamp = extract_form_data::get_selected_datetime(view_submission, ACTION_END_TIME)
        .ok_or("終了日時が選択されていません")?;

    let start_time = DateTime::<Utc>::from_timestamp(start_timestamp, 0)
        .ok_or("開始日時の変換に失敗しました")?;

    let end_time = DateTime::<Utc>::from_timestamp(end_timestamp, 0)
        .ok_or("終了日時の変換に失敗しました")?;

    let time_period = TimePeriod::new(start_time, end_time)?;

    // 備考を取得（オプション）
    let notes = extract_form_data::get_plain_text_input(view_submission, ACTION_NOTES);

    // ユーザーのメールアドレスを取得
    let identity_link = app
        .identity_repo
        .find_by_external_user_id(&ExternalSystem::Slack, &user_id.to_string())
        .await?
        .ok_or("ユーザーが登録されていません。まず /register-calendar を実行してください")?;

    let owner_email = identity_link.email().clone();

    // 予約を更新
    let update_result = app
        .update_resource_usage_usecase
        .execute(&usage_id, &owner_email, Some(time_period), notes)
        .await;

    // channel_id を取得
    let channel_id = app.user_channel_map.read().unwrap().get(&user_id).cloned();

    if let Some(channel_id) = channel_id {
        // エフェメラルメッセージで結果を送信
        let message_text = match update_result {
            Ok(_) => {
                info!(
                    "✅ リソース予約更新成功: user={}, usage_id={}",
                    user_id, usage_id_str
                );
                format!("✅ 予約を更新しました\n予約ID: {}", usage_id_str)
            }
            Err(e) => {
                error!("❌ リソース予約更新に失敗: {}", e);
                format!("❌ 更新に失敗しました: {}", e)
            }
        };

        let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
            channel_id,
            user_id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );

        let session = app.slack_client.open_session(&app.bot_token);
        session.chat_post_ephemeral(&ephemeral_req).await?;
    } else {
        error!("❌ channel_id が見つかりません");
    }

    // モーダルを閉じる
    Ok(None)
}
