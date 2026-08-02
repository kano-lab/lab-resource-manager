use crate::application::error::ApplicationError;
use crate::application::idle_notice_log::IdleNoticeLog;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::ports::{
    IdleEvidence, IdleReservation, IdleReservationNotifier, ObservationSnapshot,
    ResourceUsageObserver,
};
use crate::domain::services::resource_usage::{ReservationActivity, judge_reservation_activity};
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::mem::discriminant;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

/// 見立てがついたとき、予約者に知らせるか、まず様子を見るか
///
/// 新しい見分け方をいきなり全員へのDMとして流すと、閾値が実態に合っているかを
/// 確かめる前に人を急かすことになる。数えるところまでは同じように行い、
/// 送るかどうかだけを分ける。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticePolicy {
    /// ログに数えるだけで、予約者には知らせない
    Observe,
    /// 予約者に知らせる
    Notify,
}

/// 予約が使われていないと見なす基準
///
/// 押さえたまま計算していない予約には、プロセスすら立っていない予約より長い猶予を置く。
/// メモリを確保して待たせている状態は、立ち上げ直すと時間がかかる使い方（常駐させた
/// 推論サーバー、開いたままのノートブック）で意図的に選ばれていることがあり、
/// 手が離れているとは限らない。
#[derive(Debug, Clone, Copy)]
pub struct IdleCriteria {
    /// 予約者本人のプロセスを観測できない状態がこれだけ続いたら知らせる
    pub absent_threshold: Duration,
    /// プロセスは乗っているのに計算が走らない状態がこれだけ続いたら知らせる
    pub held_threshold: Duration,
    /// これ以上の稼働率が出ていれば、計算が走っているとみなす
    pub computing_utilization_percent: u32,
    /// 押さえたまま計算していない予約について、知らせるか様子を見るか
    pub held_notices: NoticePolicy,
    /// 観測がこれ以上途切れたら、使われていない時間を計り直す
    ///
    /// 見えていなかった間に何が起きていたかは分からない。短い欠測で計り直すと、
    /// 監視が不安定なほど誰にも知らせなくなるため、途切れの長さで線を引く。
    pub observation_gap_tolerance: Duration,
}

impl IdleCriteria {
    /// この見立てを予約者に知らせるとしたら、どの根拠でどれだけ待つか
    ///
    /// 使われている予約と、使われているかを問えない予約については何も返さない。
    fn notice_terms(&self, activity: &ReservationActivity) -> Option<(IdleEvidence, Duration)> {
        match activity {
            ReservationActivity::InUse | ReservationActivity::Undecidable => None,
            ReservationActivity::Absent => Some((IdleEvidence::NoProcesses, self.absent_threshold)),
            ReservationActivity::HeldWithoutComputing(at_rest) => Some((
                IdleEvidence::HeldWithoutComputing {
                    at_rest: at_rest.at_rest().to_vec(),
                    observed_count: at_rest.observed_count(),
                    peak_utilization_percent: at_rest.peak_utilization_percent(),
                    used_memory_mib: at_rest.used_memory_mib(),
                },
                self.held_threshold,
            )),
        }
    }

    /// この根拠を予約者に知らせるか、まず様子を見るか
    fn policy_for(&self, evidence: &IdleEvidence) -> NoticePolicy {
        match evidence {
            IdleEvidence::NoProcesses => NoticePolicy::Notify,
            IdleEvidence::HeldWithoutComputing { .. } => self.held_notices,
        }
    }
}

/// 知らせるかどうかを見たあと、実際に何をしたか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeOutcome {
    /// 予約者に知らせた
    Sent,
    /// 知らせる頃合いだったが、様子を見るにとどめた
    Withheld,
    /// まだ知らせる頃合いではない
    NotDue,
}

/// 予約が使われていない状態が、いつから、どんな様子で続いているか
#[derive(Debug, Clone)]
struct IdleSpell {
    since: DateTime<Utc>,
    /// この様子を最後に確かめた時刻（観測が途切れていないことの確認に使う）
    last_confirmed_at: DateTime<Utc>,
    evidence: IdleEvidence,
}

/// 進行中の予約のうち、予約者本人に使われていないものを検知して知らせるユースケース
///
/// # 判定
/// 予約が押さえているGPUについて、予約者本人のプロセスがひとつも観測されないか、
/// 乗ってはいるが計算が走っていない状態が続いたら、予約者へ知らせる。予約者以外の利用は
/// 無断使用として`ReconcileObservedUsagesUseCase`が扱うため、ここでは予約者本人の
/// 利用だけを見る。
///
/// # 判定しない場合
/// - 観測できていないサーバー（レポートの欠落・鮮度切れ）を含む予約
///   監視が止まっている間の沈黙は、使われていないことの証拠にならない
/// - 予約者のOSユーザー名が分からない予約
///   本人の利用を本人のものと見分けられない
/// - 部屋の予約
///   利用を観測する手段がない
/// - 残り時間が閾値に満たない予約
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
    criteria: IdleCriteria,
    notices: Arc<IdleNoticeLog>,
    /// 予約ごとの「使われていないと確かめられた最初の時刻」と、そのときの様子
    idle_spells: Mutex<HashMap<String, IdleSpell>>,
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
    /// * `criteria` - 使われていないと見なす基準
    /// * `notices` - 繰り返し声をかけないための記録（予約者の応答からも更新される）
    pub fn new(
        repository: Arc<R>,
        observer: Arc<O>,
        identity_repo: Arc<I>,
        notifier: N,
        criteria: IdleCriteria,
        notices: Arc<IdleNoticeLog>,
    ) -> Self {
        Self {
            repository,
            observer,
            identity_repo,
            notifier,
            criteria,
            notices,
            idle_spells: Mutex::new(HashMap::new()),
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

        let mut absent = 0_usize;
        let mut held = 0_usize;
        let mut held_partially = 0_usize;
        let mut notified = 0_usize;
        let mut withheld = 0_usize;
        let mut failures = 0_usize;

        for reservation in &reservations {
            let activity = self.judge(reservation, &snapshot).await?;

            if matches!(activity, ReservationActivity::InUse) {
                self.forget(reservation.id());
                continue;
            }

            let Some((evidence, threshold)) = self.criteria.notice_terms(&activity) else {
                continue;
            };

            match evidence {
                IdleEvidence::NoProcesses => absent += 1,
                IdleEvidence::HeldWithoutComputing { .. } if evidence.is_partial() => {
                    held_partially += 1
                }
                IdleEvidence::HeldWithoutComputing { .. } => held += 1,
            }

            match self
                .notify_if_due(reservation, evidence, threshold, now)
                .await
            {
                Ok(NoticeOutcome::Sent) => notified += 1,
                Ok(NoticeOutcome::Withheld) => withheld += 1,
                Ok(NoticeOutcome::NotDue) => {}
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

        self.forget_reservations_that_ended(&reservations);

        info!(
            reservations_in_progress = reservations.len(),
            servers_observed = snapshot.observed_server_count(),
            absent,
            held,
            held_partially,
            notified,
            withheld,
            failures,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "idle reservation pass finished"
        );

        Ok(())
    }

    /// 予約が予約者本人に使われているかを見立てる
    ///
    /// 観測できていないサーバーや、OSユーザー名の分からない予約者については、
    /// 見立てそのものを差し控える。
    async fn judge(
        &self,
        reservation: &ResourceUsage,
        snapshot: &ObservationSnapshot,
    ) -> Result<ReservationActivity, ApplicationError> {
        let Some(servers) = servers_of(reservation) else {
            return Ok(ReservationActivity::Undecidable);
        };

        if !servers.iter().all(|server| snapshot.covers(server)) {
            return Ok(ReservationActivity::Undecidable);
        }

        let Some(link) = self
            .identity_repo
            .find_by_email(reservation.owner_email())
            .await?
        else {
            return Ok(ReservationActivity::Undecidable);
        };

        let mut owner_identities: Vec<ExternalIdentity> = Vec::with_capacity(servers.len());
        for server in &servers {
            let system = ExternalSystem::Os {
                server: server.clone(),
            };
            let Some(identity) = link.get_identity_for_system(&system) else {
                return Ok(ReservationActivity::Undecidable);
            };
            owner_identities.push(identity.clone());
        }

        Ok(judge_reservation_activity(
            reservation.resources(),
            &owner_identities,
            snapshot,
            self.criteria.computing_utilization_percent,
        ))
    }

    /// 使われていない時間が閾値に達していれば予約者へ知らせる
    async fn notify_if_due(
        &self,
        reservation: &ResourceUsage,
        evidence: IdleEvidence,
        threshold: Duration,
        now: DateTime<Utc>,
    ) -> Result<NoticeOutcome, ApplicationError> {
        let idle_since = self.idle_since_of(reservation.id(), &evidence, now);

        if !is_worth_telling(reservation, idle_since, now, threshold) {
            return Ok(NoticeOutcome::NotDue);
        }

        if self.notices.is_silenced(reservation.id(), now) {
            return Ok(NoticeOutcome::NotDue);
        }

        if self.criteria.policy_for(&evidence) == NoticePolicy::Observe {
            info!(
                usage_id = %reservation.id().as_str(),
                idle_since = %idle_since,
                evidence = ?evidence,
                "an idle reservation would have been reported; only watching for now"
            );
            // 様子を見ている間も黙る期間は置く。毎分同じことを数え直しても分かることは増えない
            self.notices.silence(reservation.id(), now);
            return Ok(NoticeOutcome::Withheld);
        }

        info!(
            usage_id = %reservation.id().as_str(),
            owner = %reservation.owner_email().as_str(),
            idle_since = %idle_since,
            evidence = ?evidence,
            "telling the owner about an idle reservation"
        );

        self.notifier
            .notify_idle(IdleReservation::new(
                reservation.clone(),
                idle_since,
                evidence,
            ))
            .await?;
        // 知らせられてから黙る。先に記録すると、送信に失敗したまま二度と知らせなくなる
        self.notices.silence(reservation.id(), now);
        Ok(NoticeOutcome::Sent)
    }

    /// この予約が今の様子で使われていないと確かめられた最初の時刻（初めてなら今）
    ///
    /// 次の二つの場合に計り直す。
    ///
    /// 様子が変わったとき。プロセスが立っていなかった予約で推論サーバーが立ち上がったなら、
    /// それは何かが起きた合図であり、それまでの沈黙を引き継いで急かすのは筋が違う。
    ///
    /// 観測が長く途切れたとき。見えていなかった間に使われていたかもしれず、その沈黙を
    /// 使われていないことの証拠として数え続けることはできない。
    fn idle_since_of(
        &self,
        usage_id: &UsageId,
        evidence: &IdleEvidence,
        now: DateTime<Utc>,
    ) -> DateTime<Utc> {
        let mut spells = self.idle_spells.lock().unwrap();

        match spells.get_mut(usage_id.as_str()) {
            Some(spell)
                if carries_on(
                    spell,
                    evidence,
                    now,
                    self.criteria.observation_gap_tolerance,
                ) =>
            {
                spell.evidence = evidence.clone();
                spell.last_confirmed_at = now;
                spell.since
            }
            _ => {
                spells.insert(
                    usage_id.as_str().to_string(),
                    IdleSpell {
                        since: now,
                        last_confirmed_at: now,
                        evidence: evidence.clone(),
                    },
                );
                now
            }
        }
    }

    /// 使われ始めた予約を忘れる（次に使われなくなったら改めて知らせる）
    fn forget(&self, usage_id: &UsageId) {
        self.idle_spells.lock().unwrap().remove(usage_id.as_str());
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

        self.idle_spells
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

/// いま見えている様子は、記録してある使われていない時間の続きなのか
///
/// 様子が変わっていれば別の話が始まっている。観測が長く途切れていれば、その間に
/// 何があったかは分からず、続きとして数えることはできない。
fn carries_on(
    spell: &IdleSpell,
    evidence: &IdleEvidence,
    now: DateTime<Utc>,
    gap_tolerance: Duration,
) -> bool {
    discriminant(&spell.evidence) == discriminant(evidence)
        && now - spell.last_confirmed_at <= gap_tolerance
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
    use crate::domain::services::resource_usage::GpusAtRest;

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

    fn criteria() -> IdleCriteria {
        IdleCriteria {
            absent_threshold: Duration::minutes(30),
            held_threshold: Duration::hours(1),
            computing_utilization_percent: 5,
            held_notices: NoticePolicy::Notify,
            observation_gap_tolerance: Duration::minutes(5),
        }
    }

    fn gpus_at_rest(at_rest: Vec<u32>, observed_count: usize) -> GpusAtRest {
        GpusAtRest::new(
            at_rest
                .into_iter()
                .map(|device_number| {
                    Gpu::new("Thalys".to_string(), device_number, "A100".to_string())
                })
                .collect(),
            observed_count,
            3,
            Some(38_000),
        )
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

    #[test]
    fn a_reservation_being_used_is_not_something_to_tell_its_owner_about() {
        assert!(
            criteria()
                .notice_terms(&ReservationActivity::InUse)
                .is_none()
        );
        assert!(
            criteria()
                .notice_terms(&ReservationActivity::Undecidable)
                .is_none()
        );
    }

    #[test]
    fn holding_a_gpu_without_computing_is_given_a_longer_rope_than_leaving_it_untouched() {
        let criteria = criteria();

        let (_, absent) = criteria
            .notice_terms(&ReservationActivity::Absent)
            .expect("プロセスのない予約は知らせる対象");
        let (_, held) = criteria
            .notice_terms(&ReservationActivity::HeldWithoutComputing(gpus_at_rest(
                vec![0],
                1,
            )))
            .expect("計算していない予約は知らせる対象");

        assert!(
            held > absent,
            "立ち上げ直しに時間のかかる使い方を、同じ物差しで急かさない"
        );
    }

    #[test]
    fn the_evidence_carries_what_was_seen() {
        let terms = criteria().notice_terms(&ReservationActivity::HeldWithoutComputing(
            gpus_at_rest(vec![0], 1),
        ));

        assert_eq!(
            terms.map(|(evidence, _)| evidence),
            Some(IdleEvidence::HeldWithoutComputing {
                at_rest: vec![Gpu::new("Thalys".to_string(), 0, "A100".to_string())],
                observed_count: 1,
                peak_utilization_percent: 3,
                used_memory_mib: Some(38_000),
            }),
            "何を見てそう言っているのかが予約者まで届く必要がある"
        );
    }

    #[test]
    fn a_reservation_with_only_some_of_its_gpus_at_rest_is_told_apart() {
        let (partial, _) = criteria()
            .notice_terms(&ReservationActivity::HeldWithoutComputing(gpus_at_rest(
                vec![3],
                2,
            )))
            .expect("休んでいるGPUがあるなら知らせる対象");
        let (every, _) = criteria()
            .notice_terms(&ReservationActivity::HeldWithoutComputing(gpus_at_rest(
                vec![0, 1],
                2,
            )))
            .expect("休んでいるGPUがあるなら知らせる対象");

        assert!(partial.is_partial(), "2枚のうち1枚だけが休んでいる");
        assert!(!every.is_partial(), "問えた2枚とも休んでいる");
    }

    fn spell_confirmed_at(last_confirmed_at: DateTime<Utc>) -> IdleSpell {
        IdleSpell {
            since: last_confirmed_at - Duration::hours(1),
            last_confirmed_at,
            evidence: IdleEvidence::NoProcesses,
        }
    }

    #[test]
    fn a_stretch_confirmed_moments_ago_carries_on() {
        let now = Utc::now();

        assert!(carries_on(
            &spell_confirmed_at(now - Duration::minutes(1)),
            &IdleEvidence::NoProcesses,
            now,
            Duration::minutes(5)
        ));
    }

    #[test]
    fn a_stretch_that_went_unobserved_for_too_long_is_counted_afresh() {
        let now = Utc::now();

        assert!(
            !carries_on(
                &spell_confirmed_at(now - Duration::minutes(40)),
                &IdleEvidence::NoProcesses,
                now,
                Duration::minutes(5)
            ),
            "見えていなかった間の沈黙を、使われていないことの証拠として数えてはいけない"
        );
    }

    #[test]
    fn a_stretch_of_a_different_kind_is_counted_afresh() {
        let now = Utc::now();
        let held = IdleEvidence::HeldWithoutComputing {
            at_rest: vec![],
            observed_count: 1,
            peak_utilization_percent: 0,
            used_memory_mib: None,
        };

        assert!(
            !carries_on(
                &spell_confirmed_at(now - Duration::minutes(1)),
                &held,
                now,
                Duration::minutes(5)
            ),
            "プロセスが立ち上がったのは何かが起きた合図で、それまでの沈黙の続きではない"
        );
    }

    #[test]
    fn a_notice_is_withheld_while_the_criteria_only_ask_to_watch() {
        let watching = IdleCriteria {
            held_notices: NoticePolicy::Observe,
            ..criteria()
        };

        assert_eq!(
            watching.policy_for(&IdleEvidence::NoProcesses),
            NoticePolicy::Notify,
            "もとからある見分け方まで黙らせてはいけない"
        );
        assert_eq!(
            watching.policy_for(&IdleEvidence::HeldWithoutComputing {
                at_rest: vec![],
                observed_count: 1,
                peak_utilization_percent: 0,
                used_memory_mib: None,
            }),
            NoticePolicy::Observe
        );
    }
}
