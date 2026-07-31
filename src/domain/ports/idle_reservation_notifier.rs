use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};

/// 押さえられているのに使われていない予約
///
/// 予約は場所取りではなく使う約束である。使わないまま抱えられていると、
/// 他の利用者は実際には空いている時間を待つことになる。
#[derive(Debug, Clone)]
pub struct IdleReservation {
    reservation: ResourceUsage,
    idle_since: DateTime<Utc>,
}

impl IdleReservation {
    /// 新しい検知結果を作成
    ///
    /// # Arguments
    /// * `reservation` - 対象の予約
    /// * `idle_since` - 使われていないと分かっている最も早い時刻
    pub fn new(reservation: ResourceUsage, idle_since: DateTime<Utc>) -> Self {
        Self {
            reservation,
            idle_since,
        }
    }

    /// 対象の予約を取得
    pub fn reservation(&self) -> &ResourceUsage {
        &self.reservation
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
