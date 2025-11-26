//! リソース予約モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{
    ACTION_END_TIME, ACTION_GPU_DEVICE_NUMBER, ACTION_GPU_MODEL, ACTION_GPU_SERVER, ACTION_NOTES,
    ACTION_RESOURCE_TYPE, ACTION_ROOM_NAME, ACTION_START_TIME,
};
use crate::interface::slack::utility::extract_form_data;
use chrono::{DateTime, Utc};
use slack_morphism::prelude::*;
use tracing::{error, info};

/// リソース予約モーダル送信を処理
///
/// リソースを予約し、エフェメラルメッセージで結果を通知
pub async fn handle<R: ResourceUsageRepository>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("リソース予約を処理中...");

    let user_id = view_submission.user.id.clone();

    // リソースタイプを取得
    let resource_type =
        extract_form_data::get_selected_option_value(view_submission, ACTION_RESOURCE_TYPE)
            .ok_or("リソースタイプが選択されていません")?;

    // リソースタイプに応じてResourceを作成
    let resource = match resource_type.as_str() {
        "gpu" => {
            let server =
                extract_form_data::get_selected_option_value(view_submission, ACTION_GPU_SERVER)
                    .ok_or("GPUサーバーが選択されていません")?;

            let device_number_str =
                extract_form_data::get_plain_text_input(view_submission, ACTION_GPU_DEVICE_NUMBER)
                    .ok_or("GPUデバイス番号が入力されていません")?;

            let device_number: u32 = device_number_str
                .trim()
                .parse()
                .map_err(|_| "GPUデバイス番号は数値である必要があります")?;

            let model = extract_form_data::get_plain_text_input(view_submission, ACTION_GPU_MODEL)
                .ok_or("GPUモデルが入力されていません")?;

            Resource::Gpu(Gpu::new(server, device_number, model))
        }
        "room" => {
            let room_name =
                extract_form_data::get_plain_text_input(view_submission, ACTION_ROOM_NAME)
                    .ok_or("部屋名が入力されていません")?;

            Resource::Room { name: room_name }
        }
        _ => return Err("無効なリソースタイプです".into()),
    };

    // 開始・終了日時を取得
    let start_timestamp =
        extract_form_data::get_selected_datetime(view_submission, ACTION_START_TIME)
            .ok_or("開始日時が選択されていません")?;

    let end_timestamp = extract_form_data::get_selected_datetime(view_submission, ACTION_END_TIME)
        .ok_or("終了日時が選択されていません")?;

    let start_time = DateTime::<Utc>::from_timestamp(start_timestamp, 0)
        .ok_or("開始日時の変換に失敗しました")?;

    let end_time =
        DateTime::<Utc>::from_timestamp(end_timestamp, 0).ok_or("終了日時の変換に失敗しました")?;

    let time_period = TimePeriod::new(start_time, end_time)?;

    // 備考を取得（オプション）
    let notes = extract_form_data::get_plain_text_input(view_submission, ACTION_NOTES);

    // ユーザーのメールアドレスを取得
    let identity_link = app
        .identity_repo
        .find_by_external_user_id(&ExternalSystem::Slack, user_id.as_ref())
        .await?
        .ok_or("ユーザーが登録されていません。まず /register-calendar を実行してください")?;

    let owner_email = identity_link.email().clone();

    // 予約を作成
    let reservation_result = app
        .create_resource_usage_usecase
        .execute(
            owner_email.clone(),
            time_period,
            vec![resource.clone()],
            notes,
        )
        .await;

    // channel_id を取得
    let channel_id = app.user_channel_map.read().unwrap().get(&user_id).cloned();

    if let Some(channel_id) = channel_id {
        // エフェメラルメッセージで結果を送信
        let message_text = match reservation_result {
            Ok(usage_id) => {
                info!(
                    "✅ リソース予約成功: user={}, resource={}, usage_id={}",
                    user_id,
                    resource,
                    usage_id.as_str()
                );
                format!(
                    "✅ {} の予約が完了しました\n予約ID: {}",
                    resource,
                    usage_id.as_str()
                )
            }
            Err(e) => {
                error!("❌ リソース予約に失敗: {}", e);
                format!("❌ 予約に失敗しました: {}", e)
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
