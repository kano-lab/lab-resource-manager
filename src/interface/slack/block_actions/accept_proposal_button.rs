//! 事後予約提案の受諾ボタンハンドラ

use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::resource_usage::errors::ResourceConflictError;
use crate::infrastructure::reservation_proposal::ProposalAcceptPayload;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::conflict_message;
use chrono::Duration;
use slack_morphism::prelude::*;
use tracing::{error, info, warn};

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
        error!("accept button carried no proposal payload");
        return Ok(());
    };

    let Some(channel_id) = dm_channel_id(block_actions) else {
        error!("could not resolve the DM channel for the proposal");
        return Ok(());
    };

    let outcome = create_reservation_from_payload(app, value).await;
    let feedback = match &outcome {
        Ok(_) => "✅ 予約を作成しました".to_string(),
        Err(failure) => {
            error!(error = %failure, "settling the proposed reservation failed");
            build_failure_message(app, failure).await
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
            settled_message(feedback.clone()),
            message_ts,
        );
        match session.chat_update(&settled).await {
            Ok(updated) => info!(
                channel = %updated.channel,
                ts = %updated.ts,
                "cleared the proposal buttons"
            ),
            // ボタンが残るだけで予約自体は成立しているため、失敗しても処理は続ける
            Err(e) => {
                warn!(error = %e, "clearing the proposal buttons failed; the reservation still stands")
            }
        }
    }

    let post_req = SlackApiChatPostMessageRequest::new(
        channel_id,
        SlackMessageContent::new().with_text(feedback),
    );
    match session.chat_post_message(&post_req).await {
        Ok(sent) => info!(
            channel = %sent.channel,
            ts = %sent.ts,
            accepted = outcome.is_ok(),
            "sent the acceptance feedback"
        ),
        Err(e) => error!(error = %e, "sending the feedback message failed"),
    }

    Ok(())
}

/// 受諾を完了できなかった理由
///
/// ユースケースまで届いたかどうかで扱いが変わる。届いたなら`ApplicationError`をそのまま
/// 持ち、`/reserve`や`/update`と同じように競合を型で判別できる。
enum AcceptFailure {
    /// ユースケースが予約を作れなかった
    ///
    /// 受諾者を併せて持つのは、競合の相手が本人かどうかを利用者に伝えるためである。
    Rejected {
        accepted_by: EmailAddress,
        error: ApplicationError,
    },
    /// ボタンのペイロードやリソース設定を解釈できず、ユースケースまで届かなかった
    Unprocessable(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for AcceptFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected { error, .. } => write!(f, "{}", error),
            Self::Unprocessable(error) => write!(f, "{}", error),
        }
    }
}

impl<E> From<E> for AcceptFailure
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn from(error: E) -> Self {
        Self::Unprocessable(error.into())
    }
}

/// 受諾済みの提案メッセージの中身
///
/// `chat.update`は渡さなかったフィールドをそのまま残すため、`text`だけを送っても
/// 提案時の`blocks`が生き続けてボタンが押せてしまう。結果だけを載せた`blocks`で
/// 上書きして初めてボタンが消える。
fn settled_message(feedback: String) -> SlackMessageContent {
    let block = SlackSectionBlock::new().with_text(md!(feedback.clone()));
    SlackMessageContent::new()
        .with_text(feedback)
        .with_blocks(slack_blocks![some_into(block)])
}

/// 競合の相手がすべて受諾者本人か
///
/// 本人の予約と重なっているだけなら、その時間帯は既に確保できている。
/// 利用者に取れる行動がないので、失敗ではなくその旨を伝える。
fn conflicts_only_with_own_reservations(
    conflicts: &[ResourceConflictError],
    accepted_by: &EmailAddress,
) -> bool {
    conflicts
        .iter()
        .all(|conflict| conflict.existing_usage.owner_email() == accepted_by)
}

/// 受諾できなかった理由を利用者に伝える文面を組み立てる
///
/// 競合の場合、誰の予約と重なったのかが分からなければ利用者は動きようがない。
/// `/reserve`と同じ整形を通し、相手が本人であれば予約しなくてよいことを伝える。
async fn build_failure_message<R, N>(app: &SlackApp<R, N>, failure: &AcceptFailure) -> String
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let AcceptFailure::Rejected {
        accepted_by,
        error: ApplicationError::ResourceConflict { conflicts },
    } = failure
    else {
        return format!("❌ 予約の作成に失敗しました: {}", failure);
    };

    let detail =
        conflict_message::build(conflicts, app.resource_config(), app.identity_repo()).await;

    if conflicts_only_with_own_reservations(conflicts, accepted_by) {
        format!(
            "ℹ️ すでにご自身の予約があるため、事後予約は作成しませんでした\n\nこの時間帯は確保済みです。あらためて予約する必要はありません。\n\n{}",
            detail
        )
    } else {
        format!("❌ 予約を作成できませんでした\n\n{}", detail)
    }
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
) -> Result<(), AcceptFailure>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let payload: ProposalAcceptPayload = serde_json::from_str(value)?;

    info!(
        server = %payload.server,
        devices = ?payload.device_numbers,
        owner = %payload.owner_email,
        duration_minutes = payload.duration_minutes,
        "accepting a post-hoc reservation proposal"
    );

    let resources = resolve_gpu_resources(app, &payload.server, &payload.device_numbers)?;
    let owner_email = EmailAddress::new(payload.owner_email)?;

    app.accept_reservation_proposal_usecase()
        .execute(
            owner_email.clone(),
            resources,
            payload.active_since,
            Duration::minutes(payload.duration_minutes),
        )
        .await
        .map_err(|error| AcceptFailure::Rejected {
            accepted_by: owner_email,
            error,
        })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
    use crate::domain::services::resource_usage::errors::ResourceConflictError;
    use chrono::Utc;

    fn gpu(device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            "gpu-server-1".to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))
    }

    fn conflict_with(owner: &str, device_number: u32) -> ResourceConflictError {
        let now = Utc::now();
        let existing_usage = ResourceUsage::new(
            EmailAddress::new(owner.to_string()).unwrap(),
            TimePeriod::new(now, now + Duration::hours(2)).unwrap(),
            vec![gpu(device_number)],
            None,
        )
        .unwrap();

        ResourceConflictError {
            resources: vec![gpu(device_number)],
            existing_usage,
        }
    }

    fn accepter() -> EmailAddress {
        EmailAddress::new("member@example.com".to_string()).unwrap()
    }

    #[test]
    fn the_settled_message_carries_blocks_without_any_button() {
        // chat.updateは渡さなかったフィールドをそのまま残す。textだけを送っても
        // 提案時のblocksが生き続け、ボタンは消えない。blocksを送り直す必要がある。
        let content = settled_message("✅ 予約を作成しました".to_string());
        let json = serde_json::to_value(&content).unwrap();

        let blocks = json["blocks"]
            .as_array()
            .expect("blocksを送らないと提案時のボタンが残る");
        assert!(
            blocks.iter().all(|block| block["type"] != "actions"),
            "受諾後のメッセージにボタンが残っている: {:?}",
            blocks
        );
        assert!(
            serde_json::to_string(&json)
                .unwrap()
                .contains("予約を作成しました"),
            "受諾の結果が本文に出ていない: {:?}",
            json
        );
    }

    #[test]
    fn a_conflict_with_ones_own_reservation_is_recognised_as_such() {
        let conflicts = vec![
            conflict_with("member@example.com", 0),
            conflict_with("member@example.com", 1),
        ];

        assert!(conflicts_only_with_own_reservations(
            &conflicts,
            &accepter()
        ));
    }

    #[test]
    fn a_conflict_with_someone_else_is_not_ones_own() {
        let conflicts = vec![conflict_with("colleague@example.com", 0)];

        assert!(!conflicts_only_with_own_reservations(
            &conflicts,
            &accepter()
        ));
    }

    #[test]
    fn conflicting_with_both_oneself_and_someone_else_is_not_ones_own() {
        let conflicts = vec![
            conflict_with("member@example.com", 0),
            conflict_with("colleague@example.com", 1),
        ];

        assert!(!conflicts_only_with_own_reservations(
            &conflicts,
            &accepter()
        ));
    }
}
