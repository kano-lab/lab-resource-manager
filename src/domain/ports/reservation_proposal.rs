use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};

/// 未予約利用を検知した際に、利用者へ提示する事後予約の提案
///
/// 予約チャンネルへのブロードキャスト通知（`Notifier`）とは異なり、
/// 特定の利用者本人への直接的なやり取り（Slack DM等）を前提とするため別ポートとする。
/// ひとりの利用者が同じ機会に使い始めたリソースは、まとめて1件の提案として提示する。
/// リソースごとに提案を分けると、利用者は枚数分のボタンを押すことになる。
#[derive(Debug, Clone)]
pub struct ReservationProposal {
    resources: Vec<Resource>,
    owner_email: EmailAddress,
    external_identity: ExternalIdentity,
    active_since: DateTime<Utc>,
    duration_candidates: Vec<Duration>,
}

impl ReservationProposal {
    /// 新しい提案を作成
    pub fn new(
        resources: Vec<Resource>,
        owner_email: EmailAddress,
        external_identity: ExternalIdentity,
        active_since: DateTime<Utc>,
        duration_candidates: Vec<Duration>,
    ) -> Self {
        Self {
            resources,
            owner_email,
            external_identity,
            active_since,
            duration_candidates,
        }
    }

    /// 対象リソースを取得
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    /// 提案先のメールアドレスを取得
    pub fn owner_email(&self) -> &EmailAddress {
        &self.owner_email
    }

    /// 利用者の外部識別情報を取得
    pub fn external_identity(&self) -> &ExternalIdentity {
        &self.external_identity
    }

    /// 利用開始時刻を取得
    pub fn active_since(&self) -> DateTime<Utc> {
        self.active_since
    }

    /// 提案する利用時間の候補を取得
    pub fn duration_candidates(&self) -> &[Duration] {
        &self.duration_candidates
    }
}

/// 未予約利用の検知時に、利用者へ事後予約を提案するポート
///
/// 実装（Slack DM送信等）と、提案受諾後の予約作成（`CreateResourceUsageUseCase`）は分離される。
/// このポートは提案の送信にのみ責務を持つ。
#[async_trait]
pub trait ReservationProposalNotifier: Send + Sync {
    /// 利用者へ予約提案を送信
    async fn propose(&self, proposal: ReservationProposal) -> Result<(), NotificationError>;
}
