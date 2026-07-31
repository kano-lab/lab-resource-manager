use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::ports::{
    ObservedUsage, ReservationProposal, ReservationProposalNotifier, ResourceUsageObserver,
    UnauthorizedUsageNotifier,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info};

/// 同じ機会に使い始めたとみなす観測開始時刻の幅
///
/// 複数のリソースを続けて使い始めると観測開始時刻が数秒〜数十秒ずれるため、
/// この幅に収まるものはひとつの機会としてまとめて提案する。
const SESSION_GROUPING_WINDOW: Duration = Duration::minutes(5);

/// 提案済みの機会を表すキー（利用者・正規化したリソース名・開始時刻）
type ProposedSessionKey = (ExternalIdentity, Vec<String>, DateTime<Utc>);

/// 無断使用を知らせた機会を表すキー（利用者・予約ID・その機会の開始時刻）
///
/// 予約単位で数えるため、同じ予約が複数のリソースを押さえていても1回で済む。
type NotifiedUnauthorizedKey = (ExternalIdentity, String, DateTime<Utc>);

/// 実サーバーの利用状況と予約を突き合わせ、未予約利用の提案・無断使用の通知を行うユースケース
///
/// # 検知ロジック
/// - 予約と一致する利用者による利用: 何もしない（正常）
/// - 予約と異なる利用者による利用（無断使用）: `UnauthorizedUsageNotifier`で本人に直接通知する
/// - 利用者の身元が不明（IdentityLink未登録）: 無断使用かどうか原理的に判定できないためスキップする
/// - 予約が存在しない利用: `unreserved_threshold`以上継続していれば、
///   `ReservationProposalNotifier`経由で利用者本人に事後予約を提案する
///
/// # 提案のまとめ
/// 同じ利用者が近い時刻に使い始めたリソースは、ひとつの機会としてまとめて1件の提案にする。
/// リソースごとに提案を分けると、利用者は使った枚数だけボタンを押すことになる。
///
/// # 重複提案の防止
/// ひとつの機会（利用者＋リソース集合＋開始時刻）に対しては一度だけ提案する。
/// この状態はビジネス不変条件ではなくUX上のスパム防止に過ぎないため、
/// プロセス内メモリのみで保持し、永続化しない。
pub struct ReconcileObservedUsagesUseCase<R, O, I, P, U>
where
    R: ResourceUsageRepository,
    O: ResourceUsageObserver,
    I: IdentityLinkRepository,
    P: ReservationProposalNotifier,
    U: UnauthorizedUsageNotifier,
{
    repository: Arc<R>,
    observer: Arc<O>,
    identity_repo: Arc<I>,
    proposal_notifier: P,
    unauthorized_notifier: U,
    unreserved_threshold: Duration,
    duration_candidates: Vec<Duration>,
    proposed_keys: tokio::sync::Mutex<HashSet<ProposedSessionKey>>,
    notified_unauthorized_keys: tokio::sync::Mutex<HashSet<NotifiedUnauthorizedKey>>,
}

impl<R, O, I, P, U> ReconcileObservedUsagesUseCase<R, O, I, P, U>
where
    R: ResourceUsageRepository,
    O: ResourceUsageObserver,
    I: IdentityLinkRepository,
    P: ReservationProposalNotifier,
    U: UnauthorizedUsageNotifier,
{
    /// 新しいユースケースインスタンスを作成
    ///
    /// # Arguments
    /// * `unreserved_threshold` - 未予約利用を提案対象とみなす継続時間の閾値
    /// * `duration_candidates` - 提案する利用時間の候補
    pub fn new(
        repository: Arc<R>,
        observer: Arc<O>,
        identity_repo: Arc<I>,
        proposal_notifier: P,
        unauthorized_notifier: U,
        unreserved_threshold: Duration,
        duration_candidates: Vec<Duration>,
    ) -> Self {
        Self {
            repository,
            observer,
            identity_repo,
            proposal_notifier,
            unauthorized_notifier,
            unreserved_threshold,
            duration_candidates,
            proposed_keys: tokio::sync::Mutex::new(HashSet::new()),
            notified_unauthorized_keys: tokio::sync::Mutex::new(HashSet::new()),
        }
    }

    /// 一度だけポーリングを実行し、実利用と予約の突合・通知を行う
    ///
    /// # Errors
    /// 観測・リポジトリアクセス・通知送信に失敗した場合
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started_at = Utc::now();
        let snapshot = self.observer.observe_active_usages().await?;
        let observed = snapshot.usages();
        let now = started_at;
        // 突合の相手は「今この瞬間に進行中の予約」だけなので、現在時刻を含む最小の期間を問う
        let in_progress = TimePeriod::new(now, now + Duration::seconds(1))?;
        let current_usages = self.repository.find_overlapping(&in_progress).await?;

        let mut unreserved = Vec::new();
        let mut reserved = 0_usize;
        let mut failures = 0_usize;
        // 同じ予約の複数リソースを使っていても知らせるのは1回なので、予約ごとにまとめる
        let mut reserved_by_usage: HashMap<(ExternalIdentity, String), &ObservedUsage> =
            HashMap::new();

        for usage in observed {
            match Self::find_active_reservation(&current_usages, usage.resource(), now) {
                Some(reservation) => {
                    reserved += 1;
                    let key = (
                        usage.external_identity().clone(),
                        reservation.id().as_str().to_string(),
                    );
                    // 同じ機会の開始時刻として、最も早い観測を代表に選ぶ
                    reserved_by_usage
                        .entry(key)
                        .and_modify(|earliest| {
                            if usage.active_since() < earliest.active_since() {
                                *earliest = usage;
                            }
                        })
                        .or_insert(usage);
                }
                // 未予約の利用は、同じ機会に使い始めた分をまとめて提案するため一旦集める
                None if now - usage.active_since() >= self.unreserved_threshold => {
                    unreserved.push(usage.clone());
                }
                None => {}
            }
        }

        for observed_usage in reserved_by_usage.into_values() {
            let Some(reservation) =
                Self::find_active_reservation(&current_usages, observed_usage.resource(), now)
            else {
                continue;
            };
            if let Err(e) = self.reconcile_reserved(observed_usage, reservation).await {
                failures += 1;
                error!(
                    resource = %observed_usage.resource(),
                    error = %e,
                    "reconciling an observed usage failed"
                );
            }
        }

        let sessions = Self::group_into_sessions(unreserved);
        let session_count = sessions.len();

        self.forget_opportunities_that_ended(&sessions).await;
        self.forget_reservations_that_ended(&current_usages).await;

        for session in sessions {
            if let Err(e) = self.propose_for_session(&session).await {
                failures += 1;
                error!(error = %e, "sending the reservation proposal failed");
            }
        }

        // 1周の要約。個別の出来事より、件数の推移から異常に気づける
        info!(
            observed = observed.len(),
            reservations_in_progress = current_usages.len(),
            matched_to_a_reservation = reserved,
            unreserved_sessions = session_count,
            failures,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "reconcile pass finished"
        );

        Ok(())
    }

    /// 観測から消えた機会についての「提案済み」を忘れる
    ///
    /// 使い終わった機会に提案することはもうない。記録を持ち続けても増えていくだけで、
    /// 同じ機会が戻ってくることもない（使い直せば開始時刻が変わり、別の機会になる）。
    async fn forget_opportunities_that_ended(&self, sessions: &[Vec<ObservedUsage>]) {
        let still_running: HashSet<ProposedSessionKey> = sessions
            .iter()
            .filter_map(|session| {
                let first = session.first()?;
                let resources: Vec<Resource> = session
                    .iter()
                    .map(|usage| usage.resource().clone())
                    .collect();
                Some(Self::session_key(first, &resources))
            })
            .collect();

        self.proposed_keys
            .lock()
            .await
            .retain(|key| still_running.contains(key));
    }

    /// 終わった予約についての「通知済み」を忘れる
    ///
    /// 予約が終われば、その予約を巡って知らせることはもうない。
    async fn forget_reservations_that_ended(&self, in_progress: &[ResourceUsage]) {
        let still_running: HashSet<&str> = in_progress
            .iter()
            .map(|reservation| reservation.id().as_str())
            .collect();

        self.notified_unauthorized_keys
            .lock()
            .await
            .retain(|(_, usage_id, _)| still_running.contains(usage_id.as_str()));
    }

    /// 未予約の利用を「同じ機会に使い始めた一群」へ分ける
    ///
    /// 同一利用者が複数のリソースを続けて使い始めると、観測開始時刻は秒単位でずれる。
    /// 利用者ごとに時刻順へ並べ、隣接する観測が`SESSION_GROUPING_WINDOW`以内であれば
    /// 同じ機会とみなす。
    fn group_into_sessions(unreserved: Vec<ObservedUsage>) -> Vec<Vec<ObservedUsage>> {
        let mut by_user: HashMap<ExternalIdentity, Vec<ObservedUsage>> = HashMap::new();
        for usage in unreserved {
            by_user
                .entry(usage.external_identity().clone())
                .or_default()
                .push(usage);
        }

        let mut sessions = Vec::new();

        for (_user, mut usages) in by_user {
            usages.sort_by_key(|usage| usage.active_since());

            let mut current: Vec<ObservedUsage> = Vec::new();
            for usage in usages {
                let starts_new_session = current.last().is_some_and(|last| {
                    usage.active_since() - last.active_since() > SESSION_GROUPING_WINDOW
                });

                if starts_new_session {
                    sessions.push(std::mem::take(&mut current));
                }
                current.push(usage);
            }

            if !current.is_empty() {
                sessions.push(current);
            }
        }

        // 提案の順序を観測開始時刻で安定させる（利用者ごとの走査順は不定のため）
        sessions.sort_by_key(|session| session.first().map(|usage| usage.active_since()));
        sessions
    }

    fn find_active_reservation<'a>(
        usages: &'a [ResourceUsage],
        resource: &Resource,
        now: DateTime<Utc>,
    ) -> Option<&'a ResourceUsage> {
        usages.iter().find(|usage| {
            usage.time_period().start() <= now
                && now < usage.time_period().end()
                && usage.resources().iter().any(|r| r.conflicts_with(resource))
        })
    }

    /// 予約の利用者と観測された利用者が一致するか確認し、一致しなければ無断使用として通知する
    async fn reconcile_reserved(
        &self,
        observed: &ObservedUsage,
        reservation: &ResourceUsage,
    ) -> Result<(), ApplicationError> {
        // 利用者の身元が不明な場合、無断使用かどうか原理的に判定できないためスキップする
        let Some(actual_email) = self.resolve_email(observed).await? else {
            return Ok(());
        };

        if &actual_email == reservation.owner_email() {
            return Ok(());
        }

        // 同一の機会に対する再通知を防ぐ（提案と同じくUX上のスパム防止であり、
        // 送信成功後にのみ記録することで失敗時の再試行を保つ）
        let key = (
            observed.external_identity().clone(),
            reservation.id().as_str().to_string(),
            observed.active_since(),
        );
        if self.notified_unauthorized_keys.lock().await.contains(&key) {
            return Ok(());
        }

        info!(
            resource = %observed.resource(),
            actual_user = %actual_email.as_str(),
            reserved_by = %reservation.owner_email().as_str(),
            usage_id = %reservation.id().as_str(),
            "notifying unauthorized usage"
        );

        self.unauthorized_notifier
            .notify(reservation, &actual_email)
            .await?;
        self.notified_unauthorized_keys.lock().await.insert(key);
        Ok(())
    }

    /// ひとつの機会にまとめた未予約利用について、利用者へ事後予約を提案する
    async fn propose_for_session(&self, session: &[ObservedUsage]) -> Result<(), ApplicationError> {
        // 時刻順に並んでいるため、先頭がこの機会の開始時刻を表す
        let Some(first) = session.first() else {
            return Ok(());
        };

        // IdentityLink未登録の間は「提案済み」にせず、リンク後の次回ポーリングで再試行できるようにする
        let Some(owner_email) = self.resolve_email(first).await? else {
            return Ok(());
        };

        let resources: Vec<Resource> = session
            .iter()
            .map(|usage| usage.resource().clone())
            .collect();
        let key = Self::session_key(first, &resources);

        if self.proposed_keys.lock().await.contains(&key) {
            return Ok(());
        }

        info!(
            owner = %owner_email.as_str(),
            resources = %resources
                .iter()
                .map(|resource| resource.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            active_since = %first.active_since(),
            "proposing a post-hoc reservation"
        );

        let proposal = ReservationProposal::new(
            resources,
            owner_email,
            first.external_identity().clone(),
            first.active_since(),
            self.duration_candidates.clone(),
        );
        // 提案の送信（Slack DM等）が失敗しうるため、成功後にのみ「提案済み」として記録する。
        // 先に記録してしまうと、送信失敗時に永久に再試行されなくなる。
        self.proposal_notifier.propose(proposal).await?;
        self.proposed_keys.lock().await.insert(key);
        Ok(())
    }

    /// 提案の重複判定に使うキー
    ///
    /// リソースの並びは観測順に依存するため、表示名で並べ替えて正規化する。
    /// リソースが増えた場合は別の機会として扱い、改めて提案できるようにする。
    fn session_key(first: &ObservedUsage, resources: &[Resource]) -> ProposedSessionKey {
        let mut normalized: Vec<String> = resources
            .iter()
            .map(|resource| resource.to_string())
            .collect();
        normalized.sort();

        (
            first.external_identity().clone(),
            normalized,
            first.active_since(),
        )
    }

    /// 観測された外部識別情報からメールアドレスを解決する（IdentityLink未登録ならNone）
    async fn resolve_email(
        &self,
        observed: &ObservedUsage,
    ) -> Result<Option<EmailAddress>, ApplicationError> {
        let identity_link = self
            .identity_repo
            .find_by_external_user_id(
                observed.external_identity().system(),
                observed.external_identity().user_id(),
            )
            .await?;
        Ok(identity_link.map(|link| link.email().clone()))
    }
}
