//! `DetectIdleReservationsUseCase`のテスト
//!
//! 予約者本人に使われていない予約の見分け方と、知らせる/知らせない条件を検証する。

use crate::application::idle_notice_log::IdleNoticeLog;
use crate::application::usecases::detect_idle_reservations::{
    DetectIdleReservationsUseCase, IdleCriteria, NoticePolicy,
};
use crate::application::usecases::test_support::InMemoryIdentityLinkRepository;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::ports::{
    GpuActivity, IdleEvidence, ObservationSnapshot, ObservedUsage, ServerObservation,
};
use crate::infrastructure::idle_reservation_notifier::MockIdleReservationNotifier;
use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
use crate::infrastructure::resource_usage_observer::MockResourceUsageObserver;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;

const SERVER: &str = "Thalys";
/// これ以上の稼働率が出ていれば計算が走っているとみなす
const COMPUTING: u32 = 5;

type TestUseCase = DetectIdleReservationsUseCase<
    MockUsageRepository,
    MockResourceUsageObserver,
    InMemoryIdentityLinkRepository,
    MockIdleReservationNotifier,
>;

struct Fixture {
    usecase: TestUseCase,
    repository: Arc<MockUsageRepository>,
    observer: Arc<MockResourceUsageObserver>,
    identity_repo: Arc<InMemoryIdentityLinkRepository>,
    notifier: MockIdleReservationNotifier,
    notices: Arc<IdleNoticeLog>,
}

/// 使われていない予約を`threshold_minutes`で知らせる状況
fn fixture(threshold_minutes: i64) -> Fixture {
    fixture_with(criteria(threshold_minutes))
}

/// 使われていない予約を`threshold_minutes`で知らせる基準
fn criteria(threshold_minutes: i64) -> IdleCriteria {
    IdleCriteria {
        absent_threshold: Duration::minutes(threshold_minutes),
        held_threshold: Duration::minutes(threshold_minutes),
        computing_utilization_percent: COMPUTING,
        held_notices: NoticePolicy::Notify,
        observation_gap_tolerance: Duration::minutes(5),
    }
}

fn fixture_with(criteria: IdleCriteria) -> Fixture {
    let repository = Arc::new(MockUsageRepository::new());
    let observer = Arc::new(MockResourceUsageObserver::new());
    let identity_repo = Arc::new(InMemoryIdentityLinkRepository::default());
    let notifier = MockIdleReservationNotifier::new();
    let notices = Arc::new(IdleNoticeLog::new(Duration::hours(4)));

    let usecase = DetectIdleReservationsUseCase::new(
        repository.clone(),
        observer.clone(),
        identity_repo.clone(),
        notifier.clone(),
        criteria,
        notices.clone(),
    );

    Fixture {
        usecase,
        repository,
        observer,
        identity_repo,
        notifier,
        notices,
    }
}

fn os_system() -> ExternalSystem {
    ExternalSystem::Os {
        server: SERVER.to_string(),
    }
}

fn gpu(device_number: u32) -> Resource {
    Resource::Gpu(Gpu::new(
        SERVER.to_string(),
        device_number,
        "A100".to_string(),
    ))
}

fn reservation_of(owner: &str, resources: Vec<Resource>, ends_in: Duration) -> ResourceUsage {
    let now = Utc::now();
    ResourceUsage::new(
        EmailAddress::new(owner.to_string()).unwrap(),
        TimePeriod::new(now - Duration::hours(1), now + ends_in).unwrap(),
        resources,
        None,
    )
    .unwrap()
}

fn observed_by(user_id: &str, resource: Resource, active_since: DateTime<Utc>) -> ObservedUsage {
    ObservedUsage::new(
        resource,
        ExternalIdentity::new(os_system(), user_id.to_string()),
        active_since,
    )
}

/// 誰の利用も観測されなかったが、サーバーの状態は把握できている観測結果
fn nobody_is_working() -> ObservationSnapshot {
    ObservationSnapshot::new(
        Vec::new(),
        HashMap::from([(
            SERVER.to_string(),
            ServerObservation::Observed {
                generated_at: Utc::now(),
            },
        )]),
    )
}

/// あるユーザーのプロセスがGPU 0番に乗っており、その稼働率まで分かっている観測結果
fn working_at(user_id: &str, peak_utilization_percent: u32) -> ObservationSnapshot {
    working_on(user_id, vec![(0, peak_utilization_percent)])
}

/// あるユーザーのプロセスが複数のGPUに乗っており、それぞれの稼働率まで分かっている観測結果
fn working_on(user_id: &str, devices: Vec<(u32, u32)>) -> ObservationSnapshot {
    let usages = devices
        .iter()
        .map(|(device_number, _)| {
            observed_by(
                user_id,
                gpu(*device_number),
                Utc::now() - Duration::hours(1),
            )
            .with_used_memory(38_000)
        })
        .collect();

    ObservationSnapshot::new(
        usages,
        HashMap::from([(
            SERVER.to_string(),
            ServerObservation::Observed {
                generated_at: Utc::now(),
            },
        )]),
    )
    .with_gpu_activities(
        devices
            .into_iter()
            .map(|(device_number, peak)| {
                ((SERVER.to_string(), device_number), GpuActivity::new(peak))
            })
            .collect(),
    )
}

#[tokio::test]
async fn a_reservation_its_owner_is_using_is_left_alone() {
    let f = fixture(30);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    f.observer.set_active_usages(vec![observed_by(
        "owner-os",
        gpu(0),
        Utc::now() - Duration::hours(1),
    )]);

    f.usecase.poll_once().await.unwrap();

    assert!(f.notifier.sent_notices().is_empty());
}

#[tokio::test]
async fn an_unused_reservation_is_reported_once_the_threshold_passes() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();

    let notices = f.notifier.sent_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].reservation().id(), reservation.id());
}

#[tokio::test]
async fn the_owner_is_not_told_twice_about_the_same_reservation() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();
    f.usecase.poll_once().await.unwrap();
    f.usecase.poll_once().await.unwrap();

    assert_eq!(f.notifier.sent_notices().len(), 1);
}

#[tokio::test]
async fn nothing_is_said_before_the_reservation_has_been_idle_long_enough() {
    let f = fixture(30);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();

    assert!(
        f.notifier.sent_notices().is_empty(),
        "使われていないと分かった直後に急かさない"
    );
}

#[tokio::test]
async fn a_server_that_cannot_be_observed_says_nothing_about_its_reservations() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    // レポートが届かない（cron停止）サーバー
    f.observer.set_snapshot(ObservationSnapshot::new(
        Vec::new(),
        HashMap::from([(SERVER.to_string(), ServerObservation::Missing)]),
    ));

    f.usecase.poll_once().await.unwrap();

    assert!(
        f.notifier.sent_notices().is_empty(),
        "監視が止まっている間の沈黙を、使っていないことの証拠にしてはいけない"
    );
}

#[tokio::test]
async fn an_owner_without_a_linked_os_username_is_left_alone() {
    let f = fixture(0);
    // Slackは紐付いているが、このサーバーのOSユーザー名は分からない
    f.identity_repo
        .add_link("owner@example.com", ExternalSystem::Slack, "U123");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();

    assert!(
        f.notifier.sent_notices().is_empty(),
        "本人の利用を本人のものと見分けられないなら判定できない"
    );
}

#[tokio::test]
async fn someone_else_working_on_the_gpu_does_not_count_as_the_owner_using_it() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    f.identity_repo
        .add_link("guest@example.com", os_system(), "guest-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    f.observer.set_active_usages(vec![observed_by(
        "guest-os",
        gpu(0),
        Utc::now() - Duration::hours(1),
    )]);

    f.usecase.poll_once().await.unwrap();

    assert_eq!(
        f.notifier.sent_notices().len(),
        1,
        "予約者本人が使っていない以上、予約は使われていない"
    );
}

#[tokio::test]
async fn a_room_reservation_is_out_of_scope() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of(
        "owner@example.com",
        vec![Resource::Room {
            name: "会議室A".to_string(),
        }],
        Duration::hours(4),
    );
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();

    assert!(
        f.notifier.sent_notices().is_empty(),
        "部屋の利用は観測できないので、使われているかを問えない"
    );
}

#[tokio::test]
async fn a_reservation_that_is_used_again_can_be_reported_again() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    f.observer.set_snapshot(nobody_is_working());
    f.usecase.poll_once().await.unwrap();
    assert_eq!(f.notifier.sent_notices().len(), 1);

    // 予約者が使い始めた
    f.observer.set_active_usages(vec![observed_by(
        "owner-os",
        gpu(0),
        Utc::now() - Duration::minutes(5),
    )]);
    f.usecase.poll_once().await.unwrap();

    // そしてまた手が止まった
    f.observer.set_snapshot(nobody_is_working());
    f.usecase.poll_once().await.unwrap();

    assert_eq!(
        f.notifier.sent_notices().len(),
        2,
        "一度使われたなら、次に手が止まったときは改めて知らせる"
    );
}

#[tokio::test]
async fn a_failed_notice_is_retried_on_the_next_pass() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.notifier.set_failing(true);
    f.usecase.poll_once().await.unwrap();
    assert!(f.notifier.sent_notices().is_empty());

    f.notifier.set_failing(false);
    f.usecase.poll_once().await.unwrap();

    assert_eq!(
        f.notifier.sent_notices().len(),
        1,
        "送信に失敗しただけで二度と知らせなくなってはいけない"
    );
}

#[tokio::test]
async fn a_silenced_reservation_stays_quiet() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    // 予約者が「まだ使う」と答えた状況
    f.notices.silence(reservation.id(), Utc::now());

    f.usecase.poll_once().await.unwrap();

    assert!(f.notifier.sent_notices().is_empty());
}

#[tokio::test]
async fn a_gpu_held_with_memory_but_no_computation_is_reported() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    // vLLMのように、メモリは確保したまま計算が走っていない状態
    f.observer.set_snapshot(working_at("owner-os", 1));

    f.usecase.poll_once().await.unwrap();

    let notices = f.notifier.sent_notices();
    assert_eq!(
        notices.len(),
        1,
        "プロセスが乗っているだけでは、予約が使われていることにならない"
    );
    assert_eq!(
        notices[0].evidence(),
        &IdleEvidence::HeldWithoutComputing {
            at_rest: vec![Gpu::new(SERVER.to_string(), 0, "A100".to_string())],
            observed_count: 1,
            peak_utilization_percent: 1,
            used_memory_mib: Some(38_000),
        },
        "何を見てそう言っているのかが予約者まで届く"
    );
}

#[tokio::test]
async fn a_gpu_left_at_rest_is_reported_even_while_another_one_is_computing() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of(
        "owner@example.com",
        vec![gpu(0), gpu(1)],
        Duration::hours(4),
    );
    f.repository.save(&reservation).await.unwrap();

    // 2枚押さえて1枚しか回していない
    f.observer
        .set_snapshot(working_on("owner-os", vec![(0, 90), (1, 0)]));

    f.usecase.poll_once().await.unwrap();

    let notices = f.notifier.sent_notices();
    assert_eq!(
        notices.len(),
        1,
        "回っている1枚が、空いたままの1枚を覆い隠してはいけない"
    );
    assert!(
        notices[0].evidence().is_partial(),
        "一部だけが休んでいることは、全部が休んでいることとは違う"
    );
}

#[tokio::test]
async fn a_watched_criteria_counts_without_telling_the_owner() {
    let f = fixture_with(IdleCriteria {
        held_notices: NoticePolicy::Observe,
        ..criteria(0)
    });
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(working_at("owner-os", 1));

    f.usecase.poll_once().await.unwrap();

    assert!(
        f.notifier.sent_notices().is_empty(),
        "見分け方を確かめている間は、人を急かさない"
    );
}

#[tokio::test]
async fn a_watched_criteria_still_tells_the_owner_when_no_process_is_there() {
    let f = fixture_with(IdleCriteria {
        held_notices: NoticePolicy::Observe,
        ..criteria(0)
    });
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();
    f.observer.set_snapshot(nobody_is_working());

    f.usecase.poll_once().await.unwrap();

    assert_eq!(
        f.notifier.sent_notices().len(),
        1,
        "新しい見分け方を確かめることと、もとからある見分け方を止めることは別"
    );
}

#[tokio::test]
async fn a_gpu_that_is_actually_computing_is_left_alone() {
    let f = fixture(0);
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    f.observer.set_snapshot(working_at("owner-os", 90));

    f.usecase.poll_once().await.unwrap();

    assert!(f.notifier.sent_notices().is_empty());
}

#[tokio::test]
async fn a_gpu_taken_up_after_a_silent_stretch_is_given_its_own_grace_period() {
    let f = fixture_with(IdleCriteria {
        absent_threshold: Duration::zero(),
        held_threshold: Duration::minutes(30),
        ..criteria(0)
    });
    f.identity_repo
        .add_link("owner@example.com", os_system(), "owner-os");
    let reservation = reservation_of("owner@example.com", vec![gpu(0)], Duration::hours(4));
    f.repository.save(&reservation).await.unwrap();

    // プロセスが立っていないので、すぐに知らせる
    f.observer.set_snapshot(nobody_is_working());
    f.usecase.poll_once().await.unwrap();
    assert_eq!(f.notifier.sent_notices().len(), 1);

    // 予約者が推論サーバーを立ち上げた（メモリは確保したが、まだ計算は走っていない）
    f.notices.resume(reservation.id());
    f.observer.set_snapshot(working_at("owner-os", 0));
    f.usecase.poll_once().await.unwrap();

    assert_eq!(
        f.notifier.sent_notices().len(),
        1,
        "様子が変わった直後に、それまでの沈黙を持ち出して急かさない"
    );
}
