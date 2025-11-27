//! 予約更新モーダル送信ハンドラ

use crate::domain::aggregates::resource_usage::value_objects::{TimePeriod, UsageId};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::utility::datetime_parser::parse_datetime;
use crate::interface::slack::utility::extract_form_data as form_data;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::modals::result;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// 予約更新モーダル送信を処理
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("🔍 予約更新データを抽出中...");

    // Get dependencies
    let update_usage_usecase = &app.update_usage_usecase;
    let identity_repo = &app.identity_repo;

    // Get usage_id from private_metadata
    let usage_id_str = form_data::get_private_metadata(view_submission)
        .ok_or("usage_idが見つかりません（private_metadataが空です）")?;

    let usage_id = UsageId::from_string(usage_id_str.clone());
    info!("  → 更新対象の予約ID: {}", usage_id_str);

    // Get user email (for authorization check)
    let owner_email =
        user_resolver::resolve_user_email(&view_submission.user.id, identity_repo).await?;
    info!("  → ユーザー: {}", owner_email);

    // Extract form values (only date/time fields, resources cannot be changed)
    let start_date = form_data::get_selected_date(view_submission, ACTION_RESERVE_START_DATE)
        .ok_or("開始日が選択されていません")?;
    let start_time = form_data::get_selected_time(view_submission, ACTION_RESERVE_START_TIME)
        .ok_or("開始時刻が選択されていません")?;
    let end_date = form_data::get_selected_date(view_submission, ACTION_RESERVE_END_DATE)
        .ok_or("終了日が選択されていません")?;
    let end_time = form_data::get_selected_time(view_submission, ACTION_RESERVE_END_TIME)
        .ok_or("終了時刻が選択されていません")?;

    let notes = form_data::get_plain_text_input(view_submission, ACTION_RESERVE_NOTES);

    info!("📊 抽出完了");

    // Parse datetime
    let start_datetime = parse_datetime(&start_date, &start_time)?;
    let end_datetime = parse_datetime(&end_date, &end_time)?;
    info!(
        "  → 新しい期間: {} 〜 {}",
        start_datetime.format("%Y-%m-%d %H:%M"),
        end_datetime.format("%Y-%m-%d %H:%M")
    );

    let time_period = TimePeriod::new(start_datetime, end_datetime)
        .map_err(|e| format!("時間期間の作成に失敗: {}", e))?;

    // Update reservation
    info!("📝 予約を更新中...");
    match update_usage_usecase
        .execute(
            &usage_id,
            &crate::domain::common::EmailAddress::new(owner_email)?,
            Some(time_period),
            notes,
        )
        .await
    {
        Ok(_) => {
            info!("✅ 予約を更新しました: {}", usage_id.as_str());

            // 成功モーダルを返す
            let success_modal = result::create_success_modal(
                "更新完了",
                format!("予約を更新しました\n予約ID: {}", usage_id.as_str()),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: success_modal,
                },
            )))
        }
        Err(e) => {
            error!("❌ 予約更新に失敗: {}", e);

            // エラーモーダルを返す
            let error_modal = result::create_error_modal(
                "更新失敗",
                format!("予約の更新に失敗しました\n\n{}", e),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_modal },
            )))
        }
    }
}
