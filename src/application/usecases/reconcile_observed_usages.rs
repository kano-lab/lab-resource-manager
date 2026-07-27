use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
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

/// 利用者を識別するキー（外部システムの種類＋ユーザーID）
///
/// `ExternalIdentity`は紐付け日時を同一性に含むため、観測ごとに生成された値どうしでは
/// 同じ利用者と判定できない。利用者の同一判定にはこのキーを使う。
type UserKey = (ExternalSystem, String);

/// 提案済みの機会を表すキー（利用者・正規化したリソース名・開始時刻）
type ProposedSessionKey = (UserKey, Vec<String>, DateTime<Utc>);

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
    notified_unauthorized_keys: tokio::sync::Mutex<HashSet<(Resource, DateTime<Utc>)>>,
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
        let observed = self.observer.observe_active_usages().await?;
        let now = Utc::now();
        let current_usages = self.repository.find_future().await?;

        let mut unreserved = Vec::new();

        for usage in &observed {
            match Self::find_active_reservation(&current_usages, usage.resource(), now) {
                Some(reservation) => {
                    if let Err(e) = self.reconcile_reserved(usage, reservation).await {
                        error!(
                            "実利用の突合処理に失敗しました (resource={}): {}",
                            usage.resource(),
                            e
                        );
                    }
                }
                // 未予約の利用は、同じ機会に使い始めた分をまとめて提案するため一旦集める
                None if now - usage.active_since() >= self.unreserved_threshold => {
                    unreserved.push(usage.clone());
                }
                None => {}
            }
        }

        for session in Self::group_into_sessions(unreserved) {
            if let Err(e) = self.propose_for_session(&session).await {
                error!("事後予約の提案に失敗しました: {}", e);
            }
        }

        Ok(())
    }

    /// 未予約の利用を「同じ機会に使い始めた一群」へ分ける
    ///
    /// 同一利用者が複数のリソースを続けて使い始めると、観測開始時刻は秒単位でずれる。
    /// 利用者ごとに時刻順へ並べ、隣接する観測が`SESSION_GROUPING_WINDOW`以内であれば
    /// 同じ機会とみなす。
    fn group_into_sessions(unreserved: Vec<ObservedUsage>) -> Vec<Vec<ObservedUsage>> {
        let mut by_user: HashMap<UserKey, Vec<ObservedUsage>> = HashMap::new();
        for usage in unreserved {
            by_user
                .entry(Self::user_key(&usage))
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

        // 同一の観測セッションに対する再通知を防ぐ（提案と同じくUX上のスパム防止であり、
        // 送信成功後にのみ記録することで失敗時の再試行を保つ）
        let key = (observed.resource().clone(), observed.active_since());
        if self.notified_unauthorized_keys.lock().await.contains(&key) {
            return Ok(());
        }

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
            "📨 事後予約を提案: owner={}, resources=[{}], active_since={}",
            owner_email.as_str(),
            resources
                .iter()
                .map(|resource| resource.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            first.active_since()
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

        (Self::user_key(first), normalized, first.active_since())
    }

    /// 観測結果から利用者を識別するキーを作る
    fn user_key(observed: &ObservedUsage) -> UserKey {
        (
            observed.external_identity().system().clone(),
            observed.external_identity().user_id().to_string(),
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
