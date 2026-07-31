use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::ports::{
    IdleReservation, IdleReservationNotifier, ObservationSnapshot, ResourceUsageObserver,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

/// 使われていない予約について、同じ予約で繰り返し声をかけないための記録
///
/// ビジネス上の不変条件ではなく、通知がうるさくならないようにするための状態である。
/// プロセス内メモリのみで保持し永続化しない（再起動後に改めて知らせても実害はない）。
/// 予約者が「まだ使う」と答えたときにも、その予約を黙らせるために使う。
#[derive(Debug, Default)]
pub struct IdleNoticeLog {
    silenced: Mutex<HashSet<String>>,
}

impl IdleNoticeLog {
    /// この予約について当面は声をかけないようにする
    pub fn silence(&self, usage_id: &UsageId) {
        self.silenced
            .lock()
            .unwrap()
            .insert(usage_id.as_str().to_string());
    }

    /// この予約について声をかけずにいるか
    pub fn is_silenced(&self, usage_id: &UsageId) -> bool {
        self.silenced.lock().unwrap().contains(usage_id.as_str())
    }

    /// ふたたび声をかける対象に戻す
    pub fn resume(&self, usage_id: &UsageId) {
        self.silenced.lock().unwrap().remove(usage_id.as_str());
    }

    /// いま覚えておく意味のある予約についてだけ記録を残す
    ///
    /// 終わった予約について黙っていることに意味はない。放っておくと、
    /// プロセスが動き続けるあいだ記録だけが増えていく。
    pub fn retain_only(&self, is_worth_remembering: impl Fn(&str) -> bool) {
        self.silenced
            .lock()
            .unwrap()
            .retain(|usage_id| is_worth_remembering(usage_id));
    }
}

/// 予約が使われているかどうかの見立て
enum Verdict {
    /// 予約者本人の利用が観測できている
    InUse,
    /// 予約者本人の利用が観測できない
    Idle,
    /// 使われているかを問えない（観測できないサーバー、OSユーザー未リンク、部屋の予約）
    Undecidable,
}

/// 進行中の予約のうち、予約者本人に使われていないものを検知して知らせるユースケース
///
/// # 判定
/// 予約が押さえているGPU上で、予約者本人のプロセスがひとつも観測されない状態が
/// `idle_threshold`続いたら、予約者へ知らせる。予約者以外の利用は無断使用として
/// `ReconcileObservedUsagesUseCase`が扱うため、ここでは予約者本人の利用だけを見る。
///
/// # 判定しない場合
/// - 観測できていないサーバー（レポートの欠落・鮮度切れ）を含む予約
///   監視が止まっている間の沈黙は、使われていないことの証拠にならない
/// - 予約者のOSユーザー名が分からない予約
///   本人の利用を本人のものと見分けられない
/// - 部屋の予約
///   利用を観測する手段がない
/// - 残り時間が`idle_threshold`に満たない予約
///   まもなく終わる予約を急かしても、予約者に取れる手はほとんどない
pub struct DetectIdleReservationsUseCase<R, O, I, N>
where
    R: ResourceUsageRepository,
    O: ResourceUsageObserver,
    I: IdentityLinkRepository,
    N: IdleReservationNotifier,
{
    repository: Arc<R>,
    observer: Arc<O>,
    identity_repo: Arc<I>,
    notifier: N,
    idle_threshold: Duration,
    notices: Arc<IdleNoticeLog>,
    /// 予約ごとの「使われていないと確かめられた最初の時刻」
    idle_since: Mutex<HashMap<String, DateTime<Utc>>>,
}

impl<R, O, I, N> DetectIdleReservationsUseCase<R, O, I, N>
where
    R: ResourceUsageRepository,
    O: ResourceUsageObserver,
    I: IdentityLinkRepository,
    N: IdleReservationNotifier,
{
    /// 新しいユースケースインスタンスを作成
    ///
    /// # Arguments
    /// * `idle_threshold` - 使われていない状態が続いたときに知らせるまでの時間
    /// * `notices` - 繰り返し声をかけないための記録（予約者の応答からも更新される）
    pub fn new(
        repository: Arc<R>,
        observer: Arc<O>,
        identity_repo: Arc<I>,
        notifier: N,
        idle_threshold: Duration,
        notices: Arc<IdleNoticeLog>,
    ) -> Self {
        Self {
            repository,
            observer,
            identity_repo,
            notifier,
            idle_threshold,
            notices,
            idle_since: Mutex::new(HashMap::new()),
        }
    }

    /// 一度だけポーリングを実行し、使われていない予約を知らせる
    ///
    /// # Errors
    /// 観測・リポジトリアクセスに失敗した場合（個別の通知失敗は記録して続行する）
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started_at = Utc::now();
        let now = started_at;
        // 対象は「今この瞬間に進行中の予約」だけなので、現在時刻を含む最小の期間を問う
        let in_progress = TimePeriod::new(now, now + Duration::seconds(1))?;
        let reservations = self.repository.find_overlapping(&in_progress).await?;
        let snapshot = self.observer.observe_active_usages().await?;

        let mut idle = 0_usize;
        let mut notified = 0_usize;
        let mut failures = 0_usize;

        for reservation in &reservations {
            match self.judge(reservation, &snapshot).await? {
                Verdict::InUse => self.forget(reservation.id()),
                Verdict::Undecidable => {}
                Verdict::Idle => {
                    idle += 1;
                    match self.notify_if_due(reservation, now).await {
                        Ok(true) => notified += 1,
                        Ok(false) => {}
                        Err(e) => {
                            failures += 1;
                            error!(
                                usage_id = %reservation.id().as_str(),
                                error = %e,
                                "telling the owner about an idle reservation failed"
                            );
                        }
                    }
                }
            }
        }

        self.forget_reservations_that_ended(&reservations);

        info!(
            reservations_in_progress = reservations.len(),
            servers_observed = snapshot.observed_server_count(),
            idle,
            notified,
            failures,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "idle reservation pass finished"
        );

        Ok(())
    }

    /// 予約が予約者本人に使われているかを見立てる
    async fn judge(
        &self,
        reservation: &ResourceUsage,
        snapshot: &ObservationSnapshot,
    ) -> Result<Verdict, ApplicationError> {
        let Some(servers) = servers_of(reservation) else {
            return Ok(Verdict::Undecidable);
        };

        if !servers.iter().all(|server| snapshot.covers(server)) {
            return Ok(Verdict::Undecidable);
        }

        let Some(link) = self
            .identity_repo
            .find_by_email(reservation.owner_email())
            .await?
        else {
            return Ok(Verdict::Undecidable);
        };

        let mut owner_identities: Vec<ExternalIdentity> = Vec::with_capacity(servers.len());
        for server in &servers {
            let system = ExternalSystem::Os {
                server: server.clone(),
            };
            let Some(identity) = link.get_identity_for_system(&system) else {
                return Ok(Verdict::Undecidable);
            };
            owner_identities.push(identity.clone());
        }

        let in_use = snapshot.usages().iter().any(|observed| {
            owner_identities.contains(observed.external_identity())
                && reservation
                    .resources()
                    .iter()
                    .any(|reserved| reserved.conflicts_with(observed.resource()))
        });

        Ok(if in_use {
            Verdict::InUse
        } else {
            Verdict::Idle
        })
    }

    /// 使われていない時間が閾値に達していれば予約者へ知らせる
    ///
    /// 戻り値は実際に知らせたかどうか。
    async fn notify_if_due(
        &self,
        reservation: &ResourceUsage,
        now: DateTime<Utc>,
    ) -> Result<bool, ApplicationError> {
        let idle_since = self.idle_since_of(reservation.id(), now);

        if !is_worth_telling(reservation, idle_since, now, self.idle_threshold) {
            return Ok(false);
        }

        if self.notices.is_silenced(reservation.id()) {
            return Ok(false);
        }

        info!(
            usage_id = %reservation.id().as_str(),
            owner = %reservation.owner_email().as_str(),
            idle_since = %idle_since,
            "telling the owner about an idle reservation"
        );

        self.notifier
            .notify_idle(IdleReservation::new(reservation.clone(), idle_since))
            .await?;
        // 知らせられてから黙る。先に記録すると、送信に失敗したまま二度と知らせなくなる
        self.notices.silence(reservation.id());
        Ok(true)
    }

    /// この予約が使われていないと確かめられた最初の時刻（初めてなら今）
    fn idle_since_of(&self, usage_id: &UsageId, now: DateTime<Utc>) -> DateTime<Utc> {
        *self
            .idle_since
            .lock()
            .unwrap()
            .entry(usage_id.as_str().to_string())
            .or_insert(now)
    }

    /// 使われ始めた予約を忘れる（次に使われなくなったら改めて知らせる）
    fn forget(&self, usage_id: &UsageId) {
        self.idle_since.lock().unwrap().remove(usage_id.as_str());
        self.notices.resume(usage_id);
    }

    /// 進行中でなくなった予約についての記録を落とす
    ///
    /// 終わった予約に対して知らせることはもう何もない。記録を持ち続けても
    /// 増えていくだけで、次に同じ予約が現れることもない。
    fn forget_reservations_that_ended(&self, in_progress: &[ResourceUsage]) {
        let still_running: HashSet<&str> = in_progress
            .iter()
            .map(|reservation| reservation.id().as_str())
            .collect();

        self.idle_since
            .lock()
            .unwrap()
            .retain(|usage_id, _| still_running.contains(usage_id.as_str()));
        self.notices
            .retain_only(|usage_id| still_running.contains(usage_id));
    }
}

/// 予約が押さえているGPUのサーバー名（部屋を含む予約は観測の対象外）
fn servers_of(reservation: &ResourceUsage) -> Option<HashSet<String>> {
    let mut servers = HashSet::new();

    for resource in reservation.resources() {
        match resource {
            Resource::Gpu(gpu) => {
                servers.insert(gpu.server().to_string());
            }
            Resource::Room { .. } => return None,
        }
    }

    Some(servers)
}

/// 使われていない予約について、いま予約者へ知らせるべきか
///
/// 使われていない時間が閾値に達していても、残り時間が閾値に満たなければ知らせない。
/// まもなく終わる予約を急かしても、予約者が開けられる時間はほとんど残っていない。
fn is_worth_telling(
    reservation: &ResourceUsage,
    idle_since: DateTime<Utc>,
    now: DateTime<Utc>,
    threshold: Duration,
) -> bool {
    now - idle_since >= threshold && reservation.time_period().end() - now >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, TimePeriod};
    use crate::domain::common::EmailAddress;

    fn reservation_ending_in(remaining: Duration, now: DateTime<Utc>) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
            TimePeriod::new(now - Duration::hours(1), now + remaining).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[test]
    fn an_idle_stretch_shorter_than_the_threshold_is_not_worth_telling() {
        let now = Utc::now();
        let reservation = reservation_ending_in(Duration::hours(4), now);

        assert!(!is_worth_telling(
            &reservation,
            now - Duration::minutes(20),
            now,
            Duration::minutes(30)
        ));
    }

    #[test]
    fn an_idle_stretch_past_the_threshold_is_worth_telling() {
        let now = Utc::now();
        let reservation = reservation_ending_in(Duration::hours(4), now);

        assert!(is_worth_telling(
            &reservation,
            now - Duration::minutes(31),
            now,
            Duration::minutes(30)
        ));
    }

    #[test]
    fn a_reservation_about_to_end_is_not_worth_telling() {
        let now = Utc::now();
        // 1時間使われていないが、残りは10分しかない
        let reservation = reservation_ending_in(Duration::minutes(10), now);

        assert!(
            !is_worth_telling(
                &reservation,
                now - Duration::hours(1),
                now,
                Duration::minutes(30)
            ),
            "開けられる時間がほとんどない予約を急かさない"
        );
    }

    #[test]
    fn a_notice_log_keeps_only_what_is_worth_remembering() {
        let kept = UsageId::from_string("running".to_string());
        let dropped = UsageId::from_string("finished".to_string());
        let log = IdleNoticeLog::default();
        log.silence(&kept);
        log.silence(&dropped);

        log.retain_only(|usage_id| usage_id == kept.as_str());

        assert!(log.is_silenced(&kept));
        assert!(
            !log.is_silenced(&dropped),
            "終わった予約について黙り続ける意味はない"
        );
    }

    #[test]
    fn a_reservation_with_exactly_the_threshold_left_is_still_worth_telling() {
        let now = Utc::now();
        let reservation = reservation_ending_in(Duration::minutes(30), now);

        assert!(is_worth_telling(
            &reservation,
            now - Duration::minutes(30),
            now,
            Duration::minutes(30)
        ));
    }
}
