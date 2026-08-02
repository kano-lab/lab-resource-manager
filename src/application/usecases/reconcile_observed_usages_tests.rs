//! `ReconcileObservedUsagesUseCase`のテスト
//!
//! 実利用と予約の突合、無断使用の通知、事後予約提案のまとめ方を検証する。

use crate::application::usecases::reconcile_observed_usages::ReconcileObservedUsagesUseCase;
use crate::application::usecases::test_support::InMemoryIdentityLinkRepository;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::ports::{
    NotificationError, ObservedUsage, ReservationProposal, ReservationProposalNotifier,
    UnauthorizedUsageNotifier,
};
use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
use crate::infrastructure::reservation_proposal::MockReservationProposalNotifier;
use crate::infrastructure::resource_usage_observer::MockResourceUsageObserver;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

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

fn gpu_resource_device2() -> Resource {
    Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "A100".to_string()))
}

/// gpu_resource()と同じサーバー("Thalys")に紐づくOS識別子
fn os_system() -> ExternalSystem {
    ExternalSystem::Os {
        server: "Thalys".to_string(),
    }
}

fn make_reservation(owner_email: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> ResourceUsage {
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
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
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
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
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
    let (usecase, _repo, observer, _identity_repo, proposal_notifier, _notifier) = make_usecase(15);

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
    let (usecase, repo, observer, identity_repo, proposal_notifier, notifier) = make_usecase(15);
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
    let (usecase, repo, observer, identity_repo, proposal_notifier, notifier) = make_usecase(15);
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
    let (usecase, repo, observer, identity_repo, _proposal_notifier, notifier) = make_usecase(15);
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
    let (usecase, repo, observer, _identity_repo, proposal_notifier, notifier) = make_usecase(15);
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
#[tokio::test]
async fn test_same_user_starting_several_gpus_gets_one_proposal() {
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    // 3枚を続けて使い始めると、観測開始時刻は秒単位でずれる
    let active_since = Utc::now() - Duration::minutes(20);
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since + Duration::seconds(3),
        ),
        ObservedUsage::new(
            gpu_resource_device2(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since + Duration::seconds(9),
        ),
    ]);

    usecase.poll_once().await.unwrap();

    let proposals = proposal_notifier.sent_proposals();
    assert_eq!(proposals.len(), 1, "枚数分の提案に分かれてはいけない");
    assert_eq!(
        proposals[0].resources().len(),
        3,
        "3枚が1件の提案にまとまるべき"
    );
    assert_eq!(
        proposals[0].active_since(),
        active_since,
        "利用開始時刻はグループ内で最も早い時刻を使うべき"
    );
}

#[tokio::test]
async fn test_usages_far_apart_in_time_are_proposed_separately() {
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    let now = Utc::now();
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            now - Duration::minutes(40),
        ),
        // 30分後に別の作業として使い始めた分は、別の機会として扱う
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            now - Duration::minutes(20),
        ),
    ]);

    usecase.poll_once().await.unwrap();

    assert_eq!(
        proposal_notifier.sent_proposals().len(),
        2,
        "離れた時刻の利用は別の提案になるべき"
    );
}

#[tokio::test]
async fn test_reserved_resource_is_excluded_from_the_grouped_proposal() {
    let (usecase, repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    // GPU:0 は予約済み（提案対象外）、GPU:1 は未予約
    let now = Utc::now();
    let reservation = make_reservation(
        "user@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    repo.save(&reservation).await.unwrap();

    let active_since = now - Duration::minutes(20);
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
    ]);

    usecase.poll_once().await.unwrap();

    let proposals = proposal_notifier.sent_proposals();
    assert_eq!(proposals.len(), 1);
    assert_eq!(
        proposals[0].resources(),
        &[gpu_resource_device1()],
        "予約済みのリソースは提案に含めない"
    );
}

#[tokio::test]
async fn test_grouped_proposal_is_sent_only_once_per_session() {
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    let active_since = Utc::now() - Duration::minutes(20);
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
    ]);

    usecase.poll_once().await.unwrap();
    usecase.poll_once().await.unwrap();

    assert_eq!(
        proposal_notifier.sent_proposals().len(),
        1,
        "同じ観測セッションへの提案は一度だけ"
    );
}

#[tokio::test]
async fn a_reservation_that_starts_later_today_does_not_count_as_in_progress() {
    let (usecase, repo, observer, identity_repo, proposal_notifier, notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    // 同じGPUに、今日のうちに始まる予約がある（まだ進行中ではない）
    let now = Utc::now();
    let later_today = make_reservation(
        "someone-else@example.com",
        now + Duration::hours(3),
        now + Duration::hours(5),
    );
    repo.save(&later_today).await.unwrap();

    observer.set_active_usages(vec![ObservedUsage::new(
        gpu_resource(),
        ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
        now - Duration::minutes(20),
    )]);

    usecase.poll_once().await.unwrap();

    // 進行中の予約はないので、未予約利用として提案される
    let proposals = proposal_notifier.sent_proposals();
    assert_eq!(
        proposals.len(),
        1,
        "先の予約は突合の相手にならず、今の利用は未予約として扱われるべき"
    );
    assert_eq!(proposals[0].owner_email().as_str(), "user@example.com");
    // 予約者が違う相手と照合してしまうと無断使用として通知されてしまう
    assert!(
        notifier.notified().is_empty(),
        "先の予約を進行中とみなして無断使用通知を出してはいけない"
    );
}

#[tokio::test]
async fn using_several_gpus_of_one_reservation_notifies_once() {
    let (usecase, repo, observer, identity_repo, _proposal_notifier, notifier) = make_usecase(15);
    identity_repo.add_link("other@example.com", os_system(), "kkawaguchi");

    // 1件の予約が2枚のGPUを押さえていて、別の利用者がその2枚を使っている
    let now = Utc::now();
    let reservation = ResourceUsage::new(
        EmailAddress::new("owner@example.com".to_string()).unwrap(),
        TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap(),
        vec![gpu_resource(), gpu_resource_device1()],
        None,
    )
    .unwrap();
    repo.save(&reservation).await.unwrap();

    let active_since = now - Duration::minutes(20);
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since + Duration::seconds(4),
        ),
    ]);

    usecase.poll_once().await.unwrap();

    let notified = notifier.notified();
    assert_eq!(
        notified.len(),
        1,
        "ひとつの予約に対する無断使用の通知は、枚数分ではなく1回であるべき"
    );
    // 通知の本文は予約全体を示すため、2枚の情報は1通に収まる
    assert_eq!(notified[0].0.resources().len(), 2);
    assert_eq!(notified[0].1.as_str(), "other@example.com");
}

#[tokio::test]
async fn using_gpus_of_two_different_reservations_notifies_for_each() {
    let (usecase, repo, observer, identity_repo, _proposal_notifier, notifier) = make_usecase(15);
    identity_repo.add_link("other@example.com", os_system(), "kkawaguchi");

    // 別々の予約が1枚ずつ押さえている場合は、それぞれ知らせる必要がある
    let now = Utc::now();
    for (owner, resource) in [
        ("owner-a@example.com", gpu_resource()),
        ("owner-b@example.com", gpu_resource_device1()),
    ] {
        let reservation = ResourceUsage::new(
            EmailAddress::new(owner.to_string()).unwrap(),
            TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap(),
            vec![resource],
            None,
        )
        .unwrap();
        repo.save(&reservation).await.unwrap();
    }

    let active_since = now - Duration::minutes(20);
    observer.set_active_usages(vec![
        ObservedUsage::new(
            gpu_resource(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
        ObservedUsage::new(
            gpu_resource_device1(),
            ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
            active_since,
        ),
    ]);

    usecase.poll_once().await.unwrap();

    assert_eq!(
        notifier.notified().len(),
        2,
        "予約が別なら、それぞれの予約者について知らせるべき"
    );
}

#[tokio::test]
async fn an_opportunity_that_ended_is_forgotten() {
    let (usecase, _repo, observer, identity_repo, proposal_notifier, _notifier) = make_usecase(15);
    identity_repo.add_link("user@example.com", os_system(), "kkawaguchi");

    let active_since = Utc::now() - Duration::minutes(20);
    let usage = ObservedUsage::new(
        gpu_resource(),
        ExternalIdentity::new(os_system(), "kkawaguchi".to_string()),
        active_since,
    );

    observer.set_active_usages(vec![usage.clone()]);
    usecase.poll_once().await.unwrap();
    assert_eq!(proposal_notifier.sent_proposals().len(), 1);

    // 使い終わって観測から消える
    observer.set_active_usages(vec![]);
    usecase.poll_once().await.unwrap();

    // 同じ機会が観測に戻ることはないが、戻ったとしても記録は残っていない
    observer.set_active_usages(vec![usage]);
    usecase.poll_once().await.unwrap();

    assert_eq!(
        proposal_notifier.sent_proposals().len(),
        2,
        "消えた機会の記録を抱え続けない"
    );
}

#[tokio::test]
async fn the_unauthorized_notice_is_forgotten_once_the_reservation_ends() {
    let (usecase, repo, observer, identity_repo, _proposal_notifier, notifier) = make_usecase(15);
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
    assert_eq!(notifier.notified().len(), 1);

    // 予約が取り消され、進行中の予約がなくなる
    repo.delete(reservation.id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    // 同じ予約が戻れば、それは改めて知らせるべき無断使用である
    repo.save(&reservation).await.unwrap();
    usecase.poll_once().await.unwrap();

    assert_eq!(
        notifier.notified().len(),
        2,
        "終わった予約の記録を抱え続けない"
    );
}
