//! リソース予約モーダル送信ハンドラ

use crate::domain::aggregates::resource_usage::factory::SPEC_ALL;
use crate::domain::aggregates::resource_usage::value_objects::resource::{Gpu, Resource};
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::*;
use crate::interface::slack::utility::datetime_parser::parse_datetime;
use crate::interface::slack::utility::extract_form_data;
use crate::interface::slack::utility::user_resolver;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// リソース予約モーダル送信を処理
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    info!("🔍 予約フォームから値を抽出中...");

    let user_id = view_submission.user.id.clone();

    // Get dependencies
    let create_usage_usecase = app.create_resource_usage_usecase();
    let identity_repo = app.identity_repo();
    let config = app.resource_config();

    // Extract form values
    let resource_type =
        extract_form_data::get_selected_option_value(view_submission, ACTION_RESERVE_RESOURCE_TYPE)
            .ok_or("リソースタイプが選択されていません")?;
    info!("  → リソースタイプ: {}", resource_type);

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

    let notes = extract_form_data::get_plain_text_input(view_submission, ACTION_RESERVE_NOTES);

    // Parse datetime
    let start_datetime = parse_datetime(&start_date, &start_time)?;
    let end_datetime = parse_datetime(&end_date, &end_time)?;
    let time_period = crate::domain::aggregates::resource_usage::value_objects::TimePeriod::new(
        start_datetime,
        end_datetime,
    )?;
    info!("  → 期間: {} ~ {}", start_datetime, end_datetime);

    // Get owner email from user_id
    let owner_email = user_resolver::resolve_user_email(&user_id, identity_repo).await?;
    info!("  → オーナー: {}", owner_email);

    // Extract resources based on type
    let resource_type_val = resource_type.as_str();
    let resources: Vec<Resource> = if resource_type_val == "gpu" {
        // Get server name
        let server_name = extract_form_data::get_selected_option_text(
            view_submission,
            ACTION_RESERVE_SERVER_SELECT,
        )
        .ok_or("サーバーが選択されていません")?;
        info!("  → サーバー: {}", server_name);

        // Get server config
        let server_config = config
            .servers
            .iter()
            .find(|s| s.name == server_name)
            .ok_or_else(|| format!("サーバー {} が見つかりません", server_name))?;

        // Get selected devices (optional)
        let device_id_values =
            extract_form_data::get_selected_options(view_submission, ACTION_RESERVE_DEVICES);
        info!("  → 選択デバイス数: {}", device_id_values.len());

        let all_devices = || {
            server_config
                .devices
                .iter()
                .map(|device| {
                    Resource::Gpu(Gpu::new(
                        server_name.clone(),
                        device.id,
                        device.model.clone(),
                    ))
                })
                .collect()
        };

        if device_id_values.is_empty() || device_id_values.iter().any(|v| v == SPEC_ALL) {
            // 未選択 or 「全てのデバイス」選択 → サーバーの全デバイスを予約
            all_devices()
        } else {
            // Parse device IDs from values
            let mut gpu_resources = Vec::new();
            for id_str in device_id_values {
                let device_id = id_str
                    .parse::<u32>()
                    .map_err(|e| format!("デバイスIDのパースに失敗: {} ({})", id_str, e))?;
                let device = server_config
                    .devices
                    .iter()
                    .find(|d| d.id == device_id)
                    .ok_or_else(|| format!("デバイス {} が見つかりません", device_id))?;
                gpu_resources.push(Resource::Gpu(Gpu::new(
                    server_name.clone(),
                    device.id,
                    device.model.clone(),
                )));
            }
            gpu_resources
        }
    } else if resource_type_val == "room" {
        let room_name = extract_form_data::get_selected_option_text(
            view_submission,
            ACTION_RESERVE_ROOM_SELECT,
        )
        .ok_or("部屋が選択されていません")?;
        info!("  → 部屋: {}", room_name);
        vec![Resource::Room { name: room_name }]
    } else {
        return Err(format!("不明なリソースタイプ: {}", resource_type_val).into());
    };

    info!("  → リソース: {:?}", resources);

    // Create reservation
    info!("📝 予約を作成中...");
    let reservation_result = create_usage_usecase
        .execute(
            crate::domain::common::EmailAddress::new(owner_email)?,
            time_period,
            resources,
            notes,
        )
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
    let message_text = match reservation_result {
        Ok(ref usage_id) => {
            info!("✅ 予約を作成しました: {}", usage_id.as_str());
            format!(
                "✅ リソースの予約が完了しました\n予約ID: {}",
                usage_id.as_str()
            )
        }
        Err(ref e) => {
            error!("❌ 予約作成に失敗: {}", e);
            format!("❌ 予約の作成に失敗しました\n\n{}", e)
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
