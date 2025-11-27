//! リソース予約モーダル送信ハンドラ

use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::aggregates::resource_usage::value_objects::resource::{Gpu, Resource};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::utility::datetime_parser::parse_datetime;
use crate::interface::slack::utility::extract_form_data as form_data;
use crate::interface::slack::utility::resource_parser::parse_device_id;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::modals::result;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// リソース予約モーダル送信を処理
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("🔍 予約フォームから値を抽出中...");

    // Get dependencies
    let create_usage_usecase = &app.create_usage_usecase;
    let identity_repo = &app.identity_repo;
    let config = &app.resource_config;

    // Extract form values
    let resource_type =
        form_data::get_selected_option_text(view_submission, ACTION_RESERVE_RESOURCE_TYPE)
            .ok_or("リソースタイプが選択されていません")?;

    let resource_type_val = if resource_type == "GPU Server" {
        "gpu"
    } else if resource_type == "Room" {
        "room"
    } else {
        &resource_type
    };

    let server_name =
        form_data::get_selected_option_text(view_submission, ACTION_RESERVE_SERVER_SELECT);
    let room_name =
        form_data::get_selected_option_text(view_submission, ACTION_RESERVE_ROOM_SELECT);
    let device_ids = form_data::get_selected_options(view_submission, ACTION_RESERVE_DEVICES);

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

    // Get user email
    let owner_email =
        user_resolver::resolve_user_email(&view_submission.user.id, identity_repo).await?;
    info!("  → ユーザー: {}", owner_email);

    // Parse datetime
    let start_datetime = parse_datetime(&start_date, &start_time)?;
    let end_datetime = parse_datetime(&end_date, &end_time)?;
    info!(
        "  → 期間: {} 〜 {}",
        start_datetime.format("%Y-%m-%d %H:%M"),
        end_datetime.format("%Y-%m-%d %H:%M")
    );

    let time_period = TimePeriod::new(start_datetime, end_datetime)
        .map_err(|e| format!("時間期間の作成に失敗: {}", e))?;

    // Build resources
    let resources = if resource_type_val == "gpu" {
        let server_name = server_name.ok_or("GPUサーバーが選択されていません")?;

        if device_ids.is_empty() {
            return Err("デバイスが選択されていません".into());
        }

        let server_config = config
            .get_server(&server_name)
            .ok_or_else(|| format!("サーバー設定が見つかりません: {}", server_name))?;

        let mut gpu_resources = Vec::new();
        for device_text in &device_ids {
            let device_id = parse_device_id(device_text)?;

            let device_config = server_config
                .devices
                .iter()
                .find(|d| d.id == device_id)
                .ok_or_else(|| format!("デバイス {} が見つかりません", device_id))?;

            gpu_resources.push(Resource::Gpu(Gpu::new(
                server_name.clone(),
                device_id,
                device_config.model.clone(),
            )));
        }
        gpu_resources
    } else if resource_type_val == "room" {
        let room_name = room_name.ok_or("部屋が選択されていません")?;
        vec![Resource::Room { name: room_name }]
    } else {
        return Err(format!("不明なリソースタイプ: {}", resource_type_val).into());
    };

    info!("  → リソース: {:?}", resources);

    // Create reservation
    info!("📝 予約を作成中...");
    match create_usage_usecase
        .execute(
            crate::domain::common::EmailAddress::new(owner_email)?,
            time_period,
            resources,
            notes,
        )
        .await
    {
        Ok(usage_id) => {
            info!("✅ 予約を作成しました: {}", usage_id.as_str());

            // 成功モーダルを返す
            let success_modal = result::create_success_modal(
                "予約完了",
                format!(
                    "リソースの予約が完了しました\n予約ID: {}",
                    usage_id.as_str()
                ),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: success_modal,
                },
            )))
        }
        Err(e) => {
            error!("❌ 予約作成に失敗: {}", e);

            // エラーモーダルを返す
            let error_modal = result::create_error_modal(
                "予約失敗",
                format!("予約の作成に失敗しました\n\n{}", e),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_modal },
            )))
        }
    }
}
