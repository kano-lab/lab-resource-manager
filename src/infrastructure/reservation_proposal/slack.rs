//! Slack DM経由の事後予約提案実装

use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::reservation_proposal::{
    ReservationProposal, ReservationProposalNotifier,
};
use crate::infrastructure::config::ResourceStyle;
use crate::infrastructure::notifier::formatter::format_resources_styled;
use crate::infrastructure::slack_direct_message::SlackDirectMessenger;
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
/// Slackボタンの`value`は文字数上限があるため、モデル名のような復元可能な情報は持たせない。
/// モデル名は受諾時にリソース設定から引き直す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalAcceptPayload {
    /// サーバー名
    pub server: String,
    /// GPUデバイス番号（同じ機会に使い始めた分をまとめて持つ）
    pub device_numbers: Vec<u32>,
    /// 提案先のメールアドレス
    pub owner_email: String,
    /// 利用開始時刻（予約の開始時刻としてそのまま使う）
    pub active_since: chrono::DateTime<chrono::Utc>,
    /// 提案された利用時間（分）
    pub duration_minutes: i64,
}

/// Slack DM経由で事後予約を提案する実装
pub struct SlackReservationProposalNotifier {
    messenger: SlackDirectMessenger,
}

impl SlackReservationProposalNotifier {
    /// 新しい実装を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            messenger: SlackDirectMessenger::new(slack_client, bot_token, identity_repo),
        }
    }
}

#[async_trait]
impl ReservationProposalNotifier for SlackReservationProposalNotifier {
    async fn propose(&self, proposal: ReservationProposal) -> Result<(), NotificationError> {
        let gpus = collect_gpus(&proposal)?;

        let text = format!(
            "⏱️ 予約なしでリソースを使用しています\n\n💻 使用中のリソース\n{}\n\nこのまま事後予約を作成しますか？",
            format_resources_styled(proposal.resources(), ResourceStyle::Full)
        );
        let blocks = build_proposal_blocks(&proposal, &gpus, &text);

        let sent = self
            .messenger
            .send(proposal.owner_email(), text, blocks)
            .await?;

        tracing::info!(
            channel = %sent.channel,
            ts = %sent.ts,
            recipient = %proposal.owner_email().as_str(),
            active_since = %proposal.active_since(),
            "sent a reservation proposal dm"
        );

        Ok(())
    }
}

/// 提案対象のGPUを取り出す
///
/// 事後予約はGPUの実利用検知から生まれるため、対象は必ずGPUであり、
/// 同一サーバーに属している（同じサーバーの観測結果をまとめたもの）。
fn collect_gpus(proposal: &ReservationProposal) -> Result<Vec<Gpu>, NotificationError> {
    let mut gpus = Vec::with_capacity(proposal.resources().len());

    for resource in proposal.resources() {
        let Resource::Gpu(gpu) = resource else {
            return Err(NotificationError::SendFailure(
                "GPU以外のリソースの事後予約提案はサポートしていません".to_string(),
            ));
        };
        gpus.push(gpu.clone());
    }

    if gpus.is_empty() {
        return Err(NotificationError::SendFailure(
            "提案対象のリソースがありません".to_string(),
        ));
    }

    Ok(gpus)
}

/// 事後予約提案のBlock Kitメッセージを構築する（純粋関数、ユニットテスト対象）
fn build_proposal_blocks(
    proposal: &ReservationProposal,
    gpus: &[Gpu],
    text: &str,
) -> Vec<SlackBlock> {
    let server = gpus
        .first()
        .map(|gpu| gpu.server().to_string())
        .unwrap_or_default();
    let device_numbers: Vec<u32> = gpus.iter().map(|gpu| gpu.device_number()).collect();

    let buttons: Vec<serde_json::Value> = proposal
        .duration_candidates()
        .iter()
        .map(|duration| {
            let payload = ProposalAcceptPayload {
                server: server.clone(),
                device_numbers: device_numbers.clone(),
                owner_email: proposal.owner_email().as_str().to_string(),
                active_since: proposal.active_since(),
                duration_minutes: duration.num_minutes(),
            };
            let value = serde_json::to_string(&payload).unwrap_or_default();

            // action_idはブロック内で一意である必要がある（重複するとSlackがinvalid_blocksで拒否する）
            serde_json::json!({
                "type": "button",
                "text": {
                    "type": "plain_text",
                    "text": format_duration_label(*duration)
                },
                "action_id": format!(
                    "{}_{}",
                    ACTION_ACCEPT_RESERVATION_PROPOSAL,
                    duration.num_minutes()
                ),
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
        warn!(error = %e, "building the proposal message blocks failed");
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
    use crate::domain::aggregates::identity_link::value_objects::{
        ExternalIdentity, ExternalSystem,
    };
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use chrono::Utc;

    fn sample_proposal(duration_candidates: Vec<Duration>) -> ReservationProposal {
        sample_proposal_with_devices(vec![0], duration_candidates)
    }

    fn sample_proposal_with_devices(
        device_numbers: Vec<u32>,
        duration_candidates: Vec<Duration>,
    ) -> ReservationProposal {
        ReservationProposal::new(
            device_numbers
                .into_iter()
                .map(|device_number| {
                    Resource::Gpu(Gpu::new(
                        "Thalys".to_string(),
                        device_number,
                        "A100".to_string(),
                    ))
                })
                .collect(),
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
        let gpus = collect_gpus(&proposal).unwrap();

        let blocks = build_proposal_blocks(&proposal, &gpus, "test message");
        assert_eq!(blocks.len(), 2);

        let json = serde_json::to_value(&blocks).unwrap();
        let elements = json[1]["elements"].as_array().unwrap();
        assert_eq!(elements.len(), candidates.len());
    }

    #[test]
    fn test_build_proposal_blocks_action_ids_are_unique_within_block() {
        let candidates = vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)];
        let proposal = sample_proposal(candidates.clone());
        let gpus = collect_gpus(&proposal).unwrap();

        let blocks = build_proposal_blocks(&proposal, &gpus, "test message");
        let json = serde_json::to_value(&blocks).unwrap();
        let action_ids: Vec<String> = json[1]["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["action_id"].as_str().unwrap().to_string())
            .collect();

        // Slackはブロック内でaction_idが重複するとinvalid_blocksでメッセージ全体を拒否する
        let unique: std::collections::HashSet<&String> = action_ids.iter().collect();
        assert_eq!(unique.len(), candidates.len());

        // gateway側は前方一致でディスパッチするため、全IDが規定のプレフィックスを持つこと
        for id in &action_ids {
            assert!(id.starts_with(ACTION_ACCEPT_RESERVATION_PROPOSAL));
        }
    }

    #[test]
    fn test_proposal_accept_payload_round_trip() {
        let payload = ProposalAcceptPayload {
            server: "Thalys".to_string(),
            device_numbers: vec![0, 1, 4],
            owner_email: "user@example.com".to_string(),
            active_since: Utc::now(),
            duration_minutes: 120,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let decoded: ProposalAcceptPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, payload);
    }
    #[test]
    fn the_message_lists_each_resource_on_its_own_line() {
        let proposal = sample_proposal_with_devices(vec![4, 5, 6], vec![Duration::hours(1)]);

        let listed = format_resources_styled(proposal.resources(), ResourceStyle::Full);

        assert_eq!(
            listed, "Thalys / A100 / GPU:4\nThalys / A100 / GPU:5\nThalys / A100 / GPU:6",
            "枚数が増えても1行に詰めず、通知と同じ表現で並べる"
        );
    }
}
