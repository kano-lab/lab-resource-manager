//! 事後予約提案の受諾ボタンハンドラ

use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::infrastructure::reservation_proposal::ProposalAcceptPayload;
use crate::interface::slack::app::SlackApp;
use chrono::Duration;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// 事後予約提案の受諾ボタンのクリックを処理
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let Some(value) = &action.value else {
        error!("❌ 提案データが取得できませんでした");
        return Ok(());
    };

    let Some(channel_id) = dm_channel_id(block_actions) else {
        error!("❌ DMチャンネルIDが取得できませんでした");
        return Ok(());
    };

    let outcome = create_reservation_from_payload(app, value).await;
    let feedback = match &outcome {
        Ok(_) => "✅ 予約を作成しました".to_string(),
        Err(e) => {
            error!("❌ 事後予約の作成に失敗: {}", e);
            format!("❌ 予約の作成に失敗しました: {}", e)
        }
    };

    let session = app.slack_client().open_session(app.bot_token());

    // 受諾できたら提案メッセージのボタンを消す。押しても無駄だと分かるようにし、
    // 連打そのものを減らす（重複防止はユースケース側で担保している）
    if let (true, Some((message_channel, message_ts))) =
        (outcome.is_ok(), proposal_message_ref(block_actions))
    {
        let settled = SlackApiChatUpdateRequest::new(
            message_channel,
            SlackMessageContent::new().with_text(feedback.clone()),
            message_ts,
        );
        if let Err(e) = session.chat_update(&settled).await {
            // ボタンが残るだけで予約自体は成立しているため、失敗しても処理は続ける
            error!("⚠️ 提案メッセージの更新に失敗: {}", e);
        }
    }

    let post_req = SlackApiChatPostMessageRequest::new(
        channel_id,
        SlackMessageContent::new().with_text(feedback),
    );
    if let Err(e) = session.chat_post_message(&post_req).await {
        error!("❌ フィードバックメッセージ送信失敗: {}", e);
    }

    Ok(())
}

/// 提案メッセージ（ボタンが乗っているメッセージ）の位置
fn proposal_message_ref(
    block_actions: &SlackInteractionBlockActionsEvent,
) -> Option<(SlackChannelId, SlackTs)> {
    let SlackInteractionActionContainer::Message(msg) = &block_actions.container else {
        return None;
    };
    let channel_id = msg
        .channel_id
        .clone()
        .or_else(|| block_actions.channel.as_ref().map(|c| c.id.clone()))?;

    Some((channel_id, msg.message_ts.clone()))
}

fn dm_channel_id(block_actions: &SlackInteractionBlockActionsEvent) -> Option<SlackChannelId> {
    if let Some(channel) = &block_actions.channel {
        return Some(channel.id.clone());
    }
    if let SlackInteractionActionContainer::Message(msg) = &block_actions.container {
        return msg.channel_id.clone();
    }
    None
}

async fn create_reservation_from_payload<R, N>(
    app: &SlackApp<R, N>,
    value: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let payload: ProposalAcceptPayload = serde_json::from_str(value)?;

    info!(
        "📍 事後予約作成: server={}, devices={:?}, owner={}, duration_minutes={}",
        payload.server, payload.device_numbers, payload.owner_email, payload.duration_minutes
    );

    let resources = resolve_gpu_resources(app, &payload.server, &payload.device_numbers)?;
    let owner_email = EmailAddress::new(payload.owner_email)?;

    app.accept_reservation_proposal_usecase()
        .execute(
            owner_email,
            resources,
            payload.active_since,
            Duration::minutes(payload.duration_minutes),
        )
        .await?;

    Ok(())
}

/// デバイス番号からGPUリソースを復元する
///
/// モデル名はボタンのペイロードに載せていないため、リソース設定から引く。
fn resolve_gpu_resources<R, N>(
    app: &SlackApp<R, N>,
    server_name: &str,
    device_numbers: &[u32],
) -> Result<Vec<Resource>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let server = app
        .resource_config()
        .get_server(server_name)
        .ok_or_else(|| format!("サーバーが見つかりません: {}", server_name))?;

    device_numbers
        .iter()
        .map(|device_number| {
            let device = server
                .devices
                .iter()
                .find(|device| device.id == *device_number)
                .ok_or_else(|| {
                    format!("デバイス{}が{}に存在しません", device_number, server_name)
                })?;

            Ok(Resource::Gpu(Gpu::new(
                server_name.to_string(),
                *device_number,
                device.model.clone(),
            )))
        })
        .collect()
}
