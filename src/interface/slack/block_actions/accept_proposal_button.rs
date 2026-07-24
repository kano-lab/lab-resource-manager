//! 事後予約提案の受諾ボタンハンドラ

use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
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

    let feedback = match create_reservation_from_payload(app, value).await {
        Ok(_) => "✅ 予約を作成しました".to_string(),
        Err(e) => {
            error!("❌ 事後予約の作成に失敗: {}", e);
            format!("❌ 予約の作成に失敗しました: {}", e)
        }
    };

    let session = app.slack_client().open_session(app.bot_token());
    let post_req = SlackApiChatPostMessageRequest::new(
        channel_id,
        SlackMessageContent::new().with_text(feedback),
    );
    if let Err(e) = session.chat_post_message(&post_req).await {
        error!("❌ フィードバックメッセージ送信失敗: {}", e);
    }

    Ok(())
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
        "📍 事後予約作成: server={}, device={}, owner={}",
        payload.server, payload.device_number, payload.owner_email
    );

    let resource = Resource::Gpu(Gpu::new(
        payload.server,
        payload.device_number,
        payload.model,
    ));
    let owner_email = EmailAddress::new(payload.owner_email)?;
    let start = payload.active_since;
    let end = start + Duration::minutes(payload.duration_minutes);
    let time_period = TimePeriod::new(start, end)?;

    app.create_resource_usage_usecase()
        .execute(
            owner_email,
            time_period,
            vec![resource],
            Some("GPU実利用検知による事後予約".to_string()),
        )
        .await?;

    Ok(())
}
