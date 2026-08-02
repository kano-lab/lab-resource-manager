use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Gpu;
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};

/// 予約が使われていないと判断した根拠
///
/// 予約者に手を打ってもらうには、何を見てそう言っているのかまで伝わっている必要がある。
/// 心当たりのない指摘を受け取った人は、確かめようがないまま放置するほかない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleEvidence {
    /// 予約者本人のプロセスがひとつも観測できない
    NoProcesses,
    /// プロセスは乗っているが、計算が走っていない
    HeldWithoutComputing {
        /// 計算が走っていないGPU
        at_rest: Vec<Gpu>,
        /// 計算しているかを問えたGPUの数（`at_rest`と同数なら、押さえている全部が休んでいる）
        observed_count: usize,
        /// 休んでいるGPUのうち、最も高かった稼働率
        peak_utilization_percent: u32,
        /// 休んでいるGPUで予約者が確保しているメモリ量の合計（MiB、問えなければ`None`）
        used_memory_mib: Option<u64>,
    },
}

impl IdleEvidence {
    /// 押さえているGPUの一部だけが休んでいるか
    ///
    /// 全部が止まっているのと、8枚のうち5枚だけが止まっているのとでは、
    /// 予約者に伝えるべきことも、取れる手も違う。
    pub fn is_partial(&self) -> bool {
        match self {
            Self::NoProcesses => false,
            Self::HeldWithoutComputing {
                at_rest,
                observed_count,
                ..
            } => at_rest.len() < *observed_count,
        }
    }
}

/// 押さえられているのに使われていない予約
///
/// 予約は場所取りではなく使う約束である。使わないまま抱えられていると、
/// 他の利用者は実際には空いている時間を待つことになる。
#[derive(Debug, Clone)]
pub struct IdleReservation {
    reservation: ResourceUsage,
    idle_since: DateTime<Utc>,
    evidence: IdleEvidence,
}

impl IdleReservation {
    /// 新しい検知結果を作成
    ///
    /// # Arguments
    /// * `reservation` - 対象の予約
    /// * `idle_since` - 使われていないと分かっている最も早い時刻
    /// * `evidence` - そう判断した根拠
    pub fn new(
        reservation: ResourceUsage,
        idle_since: DateTime<Utc>,
        evidence: IdleEvidence,
    ) -> Self {
        Self {
            reservation,
            idle_since,
            evidence,
        }
    }

    /// 対象の予約を取得
    pub fn reservation(&self) -> &ResourceUsage {
        &self.reservation
    }

    /// 使われていないと判断した根拠を取得
    pub fn evidence(&self) -> &IdleEvidence {
        &self.evidence
    }

    /// 使われていないと分かっている最も早い時刻を取得
    ///
    /// 監視が始まる前のことは分からないため、これは「使われなくなった時刻」ではなく
    /// 「使われていないと確かめられた最初の時刻」である。
    pub fn idle_since(&self) -> DateTime<Utc> {
        self.idle_since
    }

    /// 使われないまま経った時間
    pub fn idle_for(&self, now: DateTime<Utc>) -> Duration {
        now - self.idle_since
    }
}

/// 使われていない予約を予約者本人へ知らせるポート
///
/// 予約チャンネルへのブロードキャスト通知（`Notifier`）とは異なり、
/// 特定の予約者への直接的なやり取り（Slack DM等）を前提とするため別ポートとする。
/// 知らせたあとどうするか（解放するか、そのまま使うか）は本人が決めることであり、
/// このポートは知らせることにのみ責務を持つ。
#[async_trait]
pub trait IdleReservationNotifier: Send + Sync {
    /// 使われていない予約について予約者へ知らせる
    async fn notify_idle(&self, idle: IdleReservation) -> Result<(), NotificationError>;
}
