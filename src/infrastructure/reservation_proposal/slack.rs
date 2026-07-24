//! Slack DM経由の事後予約提案実装

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::reservation_proposal::{
    ReservationProposal, ReservationProposalNotifier,
};
use async_trait::async_trait;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::*;
use std::sync::Arc;
use tracing::warn;

use crate::interface::slack::constants::ACTION_ACCEPT_RESERVATION_PROPOSAL;

/// DM内の受諾ボタンに埋め込む、予約再構築に必要な最小限のデータ
///
/// `Resource`/`TimePeriod`はドメイン層でserde非依存を保っているため、
/// Slackボタンの`value`にエンコードするための専用DTOとして定義する。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalAcceptPayload {
    /// サーバー名
    pub server: String,
    /// GPUデバイス番号
    pub device_number: u32,
    /// GPUモデル名
    pub model: String,
    /// 提案先のメールアドレス
    pub owner_email: String,
    /// 利用開始時刻（予約の開始時刻としてそのまま使う）
    pub active_since: chrono::DateTime<chrono::Utc>,
    /// 提案された利用時間（分）
    pub duration_minutes: i64,
}

/// Slack DM経由で事後予約を提案する実装
pub struct SlackReservationProposalNotifier {
    slack_client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,
    identity_repo: Arc<dyn IdentityLinkRepository>,
}

impl SlackReservationProposalNotifier {
    /// 新しい実装を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            slack_client,
            bot_token,
            identity_repo,
        }
    }

    /// 提案先のSlackユーザーIDを解決する
    async fn resolve_slack_user_id(
        &self,
        proposal: &ReservationProposal,
    ) -> Result<SlackUserId, NotificationError> {
        let identity_link = self
            .identity_repo
            .find_by_email(proposal.owner_email())
            .await
            .map_err(|e| {
                NotificationError::RepositoryError(format!("IdentityLink取得失敗: {}", e))
            })?
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "IdentityLink未登録のためDMを送信できません: {}",
                    proposal.owner_email().as_str()
                ))
            })?;

        let slack_identity = identity_link
            .get_identity_for_system(&ExternalSystem::Slack)
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "Slackアカウント未リンクのためDMを送信できません: {}",
                    proposal.owner_email().as_str()
                ))
            })?;

        Ok(SlackUserId::new(slack_identity.user_id().to_string()))
    }
}

#[async_trait]
impl ReservationProposalNotifier for SlackReservationProposalNotifier {
    async fn propose(&self, proposal: ReservationProposal) -> Result<(), NotificationError> {
        let Resource::Gpu(gpu) = proposal.resource() else {
            return Err(NotificationError::SendFailure(
                "GPU以外のリソースの事後予約提案はサポートしていません".to_string(),
            ));
        };

        let slack_user_id = self.resolve_slack_user_id(&proposal).await?;

        let session = self.slack_client.open_session(&self.bot_token);

        let open_resp = session
            .conversations_open(
                &SlackApiConversationsOpenRequest::new().with_users(vec![slack_user_id]),
            )
            .await
            .map_err(|e| {
                NotificationError::SendFailure(format!("DMチャンネルのオープンに失敗: {}", e))
            })?;

        let text = format!(
            "{} が予約なしで使用中です。事後予約を作成しますか？",
            proposal.resource()
        );
        let blocks = build_proposal_blocks(&proposal, gpu, &text);

        let post_req = SlackApiChatPostMessageRequest::new(
            open_resp.channel.id,
            SlackMessageContent::new()
                .with_text(text)
                .with_blocks(blocks),
        );

        session
            .chat_post_message(&post_req)
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack DM送信失敗: {}", e)))?;

        Ok(())
    }
}

/// 事後予約提案のBlock Kitメッセージを構築する（純粋関数、ユニットテスト対象）
fn build_proposal_blocks(
    proposal: &ReservationProposal,
    gpu: &crate::domain::aggregates::resource_usage::value_objects::Gpu,
    text: &str,
) -> Vec<SlackBlock> {
    let buttons: Vec<serde_json::Value> = proposal
        .duration_candidates()
        .iter()
        .map(|duration| {
            let payload = ProposalAcceptPayload {
                server: gpu.server().to_string(),
                device_number: gpu.device_number(),
                model: gpu.model().to_string(),
                owner_email: proposal.owner_email().as_str().to_string(),
                active_since: proposal.active_since(),
                duration_minutes: duration.num_minutes(),
            };
            let value = serde_json::to_string(&payload).unwrap_or_default();

            serde_json::json!({
                "type": "button",
                "text": {
                    "type": "plain_text",
                    "text": format_duration_label(*duration)
                },
                "action_id": ACTION_ACCEPT_RESERVATION_PROPOSAL,
                "value": value
            })
        })
        .collect();

    let blocks_json = serde_json::json!([
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": text
            }
        },
        {
            "type": "actions",
            "elements": buttons
        }
    ]);

    serde_json::from_value(blocks_json).unwrap_or_else(|e| {
        warn!("Slack blocksのデシリアライズに失敗: {}", e);
        vec![]
    })
}

/// 利用時間の候補を表示用ラベルに整形する（例: 60分 -> "1時間", 90分 -> "1時間30分"）
fn format_duration_label(duration: Duration) -> String {
    let total_minutes = duration.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    match (hours, minutes) {
        (h, 0) if h > 0 => format!("{}時間", h),
        (0, m) => format!("{}分", m),
        (h, m) => format!("{}時間{}分", h, m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use chrono::Utc;

    fn sample_proposal(duration_candidates: Vec<Duration>) -> ReservationProposal {
        ReservationProposal::new(
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            EmailAddress::new("user@example.com".to_string()).unwrap(),
            ExternalIdentity::new(
                ExternalSystem::Os {
                    server: "Thalys".to_string(),
                },
                "kkawaguchi".to_string(),
            ),
            Utc::now(),
            duration_candidates,
        )
    }

    #[test]
    fn test_format_duration_label_whole_hours() {
        assert_eq!(format_duration_label(Duration::hours(1)), "1時間");
        assert_eq!(format_duration_label(Duration::hours(8)), "8時間");
    }

    #[test]
    fn test_format_duration_label_minutes_only() {
        assert_eq!(format_duration_label(Duration::minutes(30)), "30分");
    }

    #[test]
    fn test_format_duration_label_hours_and_minutes() {
        assert_eq!(format_duration_label(Duration::minutes(90)), "1時間30分");
    }

    #[test]
    fn test_build_proposal_blocks_button_count_matches_candidates() {
        let candidates = vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)];
        let proposal = sample_proposal(candidates.clone());
        let Resource::Gpu(gpu) = proposal.resource() else {
            unreachable!()
        };

        let blocks = build_proposal_blocks(&proposal, gpu, "test message");
        assert_eq!(blocks.len(), 2);

        let json = serde_json::to_value(&blocks).unwrap();
        let elements = json[1]["elements"].as_array().unwrap();
        assert_eq!(elements.len(), candidates.len());
    }

    #[test]
    fn test_proposal_accept_payload_round_trip() {
        let payload = ProposalAcceptPayload {
            server: "Thalys".to_string(),
            device_number: 0,
            model: "A100".to_string(),
            owner_email: "user@example.com".to_string(),
            active_since: Utc::now(),
            duration_minutes: 120,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let decoded: ProposalAcceptPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, payload);
    }
}
