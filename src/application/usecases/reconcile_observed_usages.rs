use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::ports::{
    ObservedUsage, ReservationProposal, ReservationProposalNotifier, ResourceUsageObserver,
    UnauthorizedUsageNotifier,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

/// 実サーバーの利用状況と予約を突き合わせ、未予約利用の提案・無断使用の通知を行うユースケース
///
/// # 検知ロジック
/// - 予約と一致する利用者による利用: 何もしない（正常）
/// - 予約と異なる利用者による利用（無断使用）: `UnauthorizedUsageNotifier`で本人に直接通知する
/// - 利用者の身元が不明（IdentityLink未登録）: 無断使用かどうか原理的に判定できないためスキップする
/// - 予約が存在しない利用: `unreserved_threshold`以上継続していれば、
///   `ReservationProposalNotifier`経由で利用者本人に事後予約を提案する
///
/// # 重複提案の防止
/// 同一の観測セッション（リソース＋観測開始時刻）に対しては一度だけ提案する。
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
    proposed_keys: tokio::sync::Mutex<HashSet<(Resource, DateTime<Utc>)>>,
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

        for usage in &observed {
            if let Err(e) = self.reconcile_one(usage, &current_usages, now).await {
                error!(
                    "実利用の突合処理に失敗しました (resource={}): {}",
                    usage.resource(),
                    e
                );
            }
        }

        Ok(())
    }

    async fn reconcile_one(
        &self,
        observed: &ObservedUsage,
        current_usages: &[ResourceUsage],
        now: DateTime<Utc>,
    ) -> Result<(), ApplicationError> {
        match Self::find_active_reservation(current_usages, observed.resource(), now) {
            Some(reservation) => self.reconcile_reserved(observed, reservation).await,
            None => self.reconcile_unreserved(observed, now).await,
        }
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

    /// 未予約利用が閾値を超えて継続していれば、利用者へ事後予約を提案する
    async fn reconcile_unreserved(
        &self,
        observed: &ObservedUsage,
        now: DateTime<Utc>,
    ) -> Result<(), ApplicationError> {
        if now - observed.active_since() < self.unreserved_threshold {
            return Ok(());
        }

        // IdentityLink未登録の間は「提案済み」にせず、リンク後の次回ポーリングで再試行できるようにする
        let Some(owner_email) = self.resolve_email(observed).await? else {
            return Ok(());
        };

        if self.already_proposed(observed).await {
            return Ok(());
        }

        let proposal = ReservationProposal::new(
            observed.resource().clone(),
            owner_email,
            observed.external_identity().clone(),
            observed.active_since(),
            self.duration_candidates.clone(),
        );
        // 提案の送信（Slack DM等）が失敗しうるため、成功後にのみ「提案済み」として記録する。
        // 先に記録してしまうと、送信失敗時に永久に再試行されなくなる。
        self.proposal_notifier.propose(proposal).await?;
        self.mark_proposed(observed).await;
        Ok(())
    }

    /// 既に提案済みかどうかを確認
    async fn already_proposed(&self, observed: &ObservedUsage) -> bool {
        let key = (observed.resource().clone(), observed.active_since());
        self.proposed_keys.lock().await.contains(&key)
    }

    /// 提案済みとして記録する
    async fn mark_proposed(&self, observed: &ObservedUsage) {
        let key = (observed.resource().clone(), observed.active_since());
        self.proposed_keys.lock().await.insert(key);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::{
        entity::IdentityLink,
        value_objects::{ExternalIdentity, ExternalSystem},
    };
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, TimePeriod};
    use crate::domain::ports::NotificationError;
    use crate::domain::ports::repositories::RepositoryError;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use crate::infrastructure::reservation_proposal::MockReservationProposalNotifier;
    use crate::infrastructure::resource_usage_observer::MockResourceUsageObserver;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct InMemoryIdentityLinkRepository {
        links: StdMutex<HashMap<String, IdentityLink>>,
    }

    impl InMemoryIdentityLinkRepository {
        fn add_link(&self, email: &str, system: ExternalSystem, user_id: &str) {
            let email_addr = EmailAddress::new(email.to_string()).unwrap();
            let identity = ExternalIdentity::new(system, user_id.to_string());
            let link = IdentityLink::with_external_identity(email_addr.clone(), identity);
            self.links
                .lock()
                .unwrap()
                .insert(email_addr.as_str().to_string(), link);
        }
    }

    #[async_trait]
    impl IdentityLinkRepository for InMemoryIdentityLinkRepository {
        async fn find_by_email(
            &self,
            email: &EmailAddress,
        ) -> Result<Option<IdentityLink>, RepositoryError> {
            Ok(self.links.lock().unwrap().get(email.as_str()).cloned())
        }

        async fn find_by_external_user_id(
            &self,
            system: &ExternalSystem,
            user_id: &str,
        ) -> Result<Option<IdentityLink>, RepositoryError> {
            let links = self.links.lock().unwrap();
            let found = links.values().find(|link| {
                link.get_identity_for_system(system)
                    .is_some_and(|identity| identity.user_id() == user_id)
            });
            Ok(found.cloned())
        }

        async fn save(&self, identity_link: IdentityLink) -> Result<(), RepositoryError> {
            self.links
                .lock()
                .unwrap()
                .insert(identity_link.email().as_str().to_string(), identity_link);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingUnauthorizedUsageNotifier {
        notified: Arc<StdMutex<Vec<(ResourceUsage, EmailAddress)>>>,
    }

    impl RecordingUnauthorizedUsageNotifier {
        fn notified(&self) -> Vec<(ResourceUsage, EmailAddress)> {
            self.notified.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl UnauthorizedUsageNotifier for RecordingUnauthorizedUsageNotifier {
        async fn notify(
            &self,
            reserved_usage: &ResourceUsage,
            actual_user_email: &EmailAddress,
        ) -> Result<(), NotificationError> {
            self.notified
                .lock()
                .unwrap()
                .push((reserved_usage.clone(), actual_user_email.clone()));
            Ok(())
        }
    }

    /// 指定したメールアドレス宛の提案だけを失敗させられるテスト用ダブル
    #[derive(Clone, Default)]
    struct FailingReservationProposalNotifier {
        fail_for_emails: Arc<StdMutex<HashSet<String>>>,
        succeeded_emails: Arc<StdMutex<Vec<String>>>,
        call_count: Arc<StdMutex<u32>>,
    }

    impl FailingReservationProposalNotifier {
        fn new() -> Self {
            Self::default()
        }

        fn fail_for(&self, email: &str) {
            self.fail_for_emails
                .lock()
                .unwrap()
                .insert(email.to_string());
        }

        fn allow(&self, email: &str) {
            self.fail_for_emails.lock().unwrap().remove(email);
        }

        fn call_count(&self) -> u32 {
            *self.call_count.lock().unwrap()
        }

        fn succeeded_emails(&self) -> Vec<String> {
            self.succeeded_emails.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ReservationProposalNotifier for FailingReservationProposalNotifier {
        async fn propose(&self, proposal: ReservationProposal) -> Result<(), NotificationError> {
            *self.call_count.lock().unwrap() += 1;
            let email = proposal.owner_email().as_str().to_string();
            if self.fail_for_emails.lock().unwrap().contains(&email) {
                return Err(NotificationError::SendFailure(
                    "test induced failure".to_string(),
                ));
            }
            self.succeeded_emails.lock().unwrap().push(email);
            Ok(())
        }
    }

    fn gpu_resource() -> Resource {
        Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()))
    }

    fn gpu_resource_device1() -> Resource {
        Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string()))
    }

    /// gpu_resource()と同じサーバー("Thalys")に紐づくOS識別子
    fn os_system() -> ExternalSystem {
        ExternalSystem::Os {
            server: "Thalys".to_string(),
        }
    }

    fn make_reservation(
        owner_email: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> ResourceUsage {
        let email = EmailAddress::new(owner_email.to_string()).unwrap();
        let period = TimePeriod::new(start, end).unwrap();
        ResourceUsage::new(email, period, vec![gpu_resource()], None).unwrap()
    }

    #[allow(clippy::type_complexity)]
    fn make_usecase(
        threshold_minutes: i64,
    ) -> (
        ReconcileObservedUsagesUseCase<
            MockUsageRepository,
            MockResourceUsageObserver,
            InMemoryIdentityLinkRepository,
            MockReservationProposalNotifier,
            RecordingUnauthorizedUsageNotifier,
        >,
        Arc<MockUsageRepository>,
        Arc<MockResourceUsageObserver>,
        Arc<InMemoryIdentityLinkRepository>,
        MockReservationProposalNotifier,
        RecordingUnauthorizedUsageNotifier,
    ) {
        let repository = Arc::new(MockUsageRepository::new());
        let observer = Arc::new(MockResourceUsageObserver::new());
        let identity_repo = Arc::new(InMemoryIdentityLinkRepository::default());
        let proposal_notifier = MockReservationProposalNotifier::new();
        let notifier = RecordingUnauthorizedUsageNotifier::default();

        let usecase = ReconcileObservedUsagesUseCase::new(
            repository.clone(),
            observer.clone(),
            identity_repo.clone(),
            proposal_notifier.clone(),
            notifier.clone(),
            Duration::minutes(threshold_minutes),
            vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)],
        );

        (
            usecase,
            repository,
            observer,
            identity_repo,
            proposal_notifier,
            notifier,
        )
    }

    #[allow(clippy::type_complexity)]
    fn make_usecase_with_failing_notifier(
        threshold_minutes: i64,
    ) -> (
        ReconcileObservedUsagesUseCase<
            MockUsageRepository,
            MockResourceUsageObserver,
            InMemoryIdentityLinkRepository,
            FailingReservationProposalNotifier,
            RecordingUnauthorizedUsageNotifier,
        >,
        Arc<MockResourceUsageObserver>,
        Arc<InMemoryIdentityLinkRepository>,
        FailingReservationProposalNotifier,
    ) {
        let repository = Arc::new(MockUsageRepository::new());
        let observer = Arc::new(MockResourceUsageObserver::new());
        let identity_repo = Arc::new(InMemoryIdentityLinkRepository::default());
        let proposal_notifier = FailingReservationProposalNotifier::new();
        let notifier = RecordingUnauthorizedUsageNotifier::default();

        let usecase = ReconcileObservedUsagesUseCase::new(
            repository,
            observer.clone(),
            identity_repo.clone(),
            proposal_notifier.clone(),
            notifier,
            Duration::minutes(threshold_minutes),
            vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)],
        );

        (usecase, observer, identity_repo, proposal_notifier)
    }

    #[tokio::test]
    async fn test_below_threshold_does_not_propose() {
        let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) =
            make_usecase(15);
        identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

        let now = Utc::now();
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            now - Duration::minutes(5),
        )]);

        usecase.poll_once().await.unwrap();

        assert!(proposal_notifier.sent_proposals().is_empty());
    }

    #[tokio::test]
    async fn test_unreserved_past_threshold_proposes_once() {
        let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) =
            make_usecase(15);
        identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

        let active_since = Utc::now() - Duration::minutes(20);
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        )]);

        usecase.poll_once().await.unwrap();
        usecase.poll_once().await.unwrap();

        let proposals = proposal_notifier.sent_proposals();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].owner_email().as_str(), "user@example.com");
        assert_eq!(proposals[0].duration_candidates().len(), 3);
    }

    #[tokio::test]
    async fn test_unlinked_user_does_not_propose() {
        let (usecase, _repo, observer, _identity_repo, proposal_notifier, _notifier) =
            make_usecase(15);

        let active_since = Utc::now() - Duration::minutes(20);
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "unknown".to_string()),
            active_since,
        )]);

        usecase.poll_once().await.unwrap();

        assert!(proposal_notifier.sent_proposals().is_empty());
    }

    #[tokio::test]
    async fn test_reserved_matching_owner_no_notification() {
        let (usecase, repo, observer, identity_repo, proposal_notifier, notifier) =
            make_usecase(15);
        identity_repo.add_link("owner@example.com", os_system(), "kkawaguchi");

        let now = Utc::now();
        let reservation = make_reservation(
            "owner@example.com",
            now - Duration::hours(1),
            now + Duration::hours(1),
        );
        repo.save(&reservation).await.unwrap();

        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            now - Duration::minutes(30),
        )]);

        usecase.poll_once().await.unwrap();

        assert!(notifier.notified().is_empty());
        assert!(proposal_notifier.sent_proposals().is_empty());
    }

    #[tokio::test]
    async fn test_reserved_mismatched_owner_notifies_unauthorized() {
        let (usecase, repo, observer, identity_repo, proposal_notifier, notifier) =
            make_usecase(15);
        identity_repo.add_link("other@example.com", os_system(), "kkawaguchi");

        let now = Utc::now();
        let reservation = make_reservation(
            "owner@example.com",
            now - Duration::hours(1),
            now + Duration::hours(1),
        );
        repo.save(&reservation).await.unwrap();

        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            now - Duration::minutes(30),
        )]);

        usecase.poll_once().await.unwrap();

        let notified = notifier.notified();
        assert_eq!(notified.len(), 1);
        let (reserved_usage, actual_user_email) = &notified[0];
        assert_eq!(reserved_usage.owner_email().as_str(), "owner@example.com");
        assert_eq!(actual_user_email.as_str(), "other@example.com");

        // 未予約提案は発生しない（既に予約が存在するため）
        assert!(proposal_notifier.sent_proposals().is_empty());
    }

    #[tokio::test]
    async fn test_unauthorized_notification_deduplicated_per_observation_session() {
        let (usecase, repo, observer, identity_repo, _proposal_notifier, notifier) =
            make_usecase(15);
        identity_repo.add_link("other@example.com", os_system(), "kkawaguchi");

        let now = Utc::now();
        let reservation = make_reservation(
            "owner@example.com",
            now - Duration::hours(1),
            now + Duration::hours(1),
        );
        repo.save(&reservation).await.unwrap();

        let active_since = now - Duration::minutes(30);
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        )]);

        // 同一の観測セッションが続く限り、何度ポーリングしても通知は1回だけ
        usecase.poll_once().await.unwrap();
        usecase.poll_once().await.unwrap();
        usecase.poll_once().await.unwrap();
        assert_eq!(notifier.notified().len(), 1);

        // プロセスが入れ替わる（観測開始時刻が変わる）と新しいセッションとして再通知する
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since + Duration::minutes(10),
        )]);
        usecase.poll_once().await.unwrap();
        assert_eq!(notifier.notified().len(), 2);
    }

    #[tokio::test]
    async fn test_reserved_unknown_identity_skips_notification() {
        let (usecase, repo, observer, _identity_repo, proposal_notifier, notifier) =
            make_usecase(15);
        // 誰にもリンクされていないOS識別子（IdentityLink未登録）

        let now = Utc::now();
        let reservation = make_reservation(
            "owner@example.com",
            now - Duration::hours(1),
            now + Duration::hours(1),
        );
        repo.save(&reservation).await.unwrap();

        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "unknown".to_string()),
            now - Duration::minutes(30),
        )]);

        usecase.poll_once().await.unwrap();

        // 利用者の身元が不明なため、無断使用かどうか原理的に判定できずスキップされる
        assert!(notifier.notified().is_empty());
        assert!(proposal_notifier.sent_proposals().is_empty());
    }

    #[tokio::test]
    async fn test_propose_failure_allows_retry_on_next_poll() {
        let (usecase, observer, identity_repo, proposal_notifier) =
            make_usecase_with_failing_notifier(15);
        identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");
        proposal_notifier.fail_for("user@example.com");

        let active_since = Utc::now() - Duration::minutes(20);
        observer.set_active_usages(vec![ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        )]);

        // 1回目: propose()が失敗する→「提案済み」として記録されない
        usecase.poll_once().await.unwrap();
        assert_eq!(proposal_notifier.call_count(), 1);
        assert!(proposal_notifier.succeeded_emails().is_empty());

        // 2回目: 送信が成功するようにしてから再度ポーリング→再試行される
        proposal_notifier.allow("user@example.com");
        usecase.poll_once().await.unwrap();
        assert_eq!(proposal_notifier.call_count(), 2);
        assert_eq!(
            proposal_notifier.succeeded_emails(),
            vec!["user@example.com".to_string()]
        );
    }

    #[tokio::test]
    async fn test_one_failure_does_not_block_other_usages() {
        let (usecase, observer, identity_repo, proposal_notifier) =
            make_usecase_with_failing_notifier(15);
        identity_repo.add_link("userA@example.com", os_system(), "kkawaguchiA");
        identity_repo.add_link("userB@example.com", os_system(), "kkawaguchiB");
        proposal_notifier.fail_for("userA@example.com");

        let active_since = Utc::now() - Duration::minutes(20);
        observer.set_active_usages(vec![
            ObservedUsage::new(
                gpu_resource(),
                ExternalIdentity::new(os_system(), "kkawaguchiA".to_string()),
                active_since,
            ),
            ObservedUsage::new(
                gpu_resource_device1(),
                ExternalIdentity::new(os_system(), "kkawaguchiB".to_string()),
                active_since,
            ),
        ]);

        usecase.poll_once().await.unwrap();

        assert_eq!(proposal_notifier.call_count(), 2);
        assert_eq!(
            proposal_notifier.succeeded_emails(),
            vec!["userB@example.com".to_string()]
        );
    }
}
