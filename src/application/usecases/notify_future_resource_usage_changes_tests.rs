//! `NotifyFutureResourceUsageChangesUseCase`のテスト
//!
//! 定期予約が回の数だけの通知にならず、1つの出来事として伝わることを検証する。

use super::*;
use crate::domain::aggregates::recurring_reservation::{
    RecurrenceEnd, RecurrenceFrequency, RecurrenceRule,
};
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, UsageId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
use async_trait::async_trait;
use std::sync::Mutex;

const WATCH_WINDOW_DAYS: i64 = 60;

/// 受け取った通知を記録するテストダブル
#[derive(Clone, Default)]
struct RecordingNotifier {
    events: Arc<Mutex<Vec<NotificationEvent>>>,
}

impl RecordingNotifier {
    /// これまでに受け取った通知を取り出し、記録を空にする
    fn take(&self) -> Vec<NotificationEvent> {
        std::mem::take(&mut self.events.lock().unwrap())
    }
}

#[async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, event: NotificationEvent) -> Result<(), NotificationError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

fn series_id(id: &str) -> RecurringReservationId {
    RecurringReservationId::from_string(id.to_string())
}

fn usage(id: &str, start: DateTime<Utc>) -> ResourceUsage {
    ResourceUsage::reconstruct(
        UsageId::from_string(id.to_string()),
        EmailAddress::new("owner@example.com".to_string()).unwrap(),
        TimePeriod::new(start, start + Duration::hours(2)).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        None,
    )
    .unwrap()
}

/// 定期予約`series`の、今から`days_ahead`日後の回
fn occurrence(series: &str, days_ahead: i64) -> ResourceUsage {
    usage(
        &format!("{series}-{days_ahead}"),
        Utc::now() + Duration::days(days_ahead),
    )
    .as_occurrence_of(series_id(series))
}

/// 毎週の定期予約を、監視範囲に入る回とともに登録する
async fn put_weekly_series(
    repository: &MockUsageRepository,
    series: &str,
    days_ahead: &[i64],
) -> Vec<ResourceUsage> {
    let occurrences: Vec<ResourceUsage> = days_ahead
        .iter()
        .map(|days| occurrence(series, *days))
        .collect();
    for occurrence in &occurrences {
        repository.save(occurrence).await.unwrap();
    }
    repository.put_recurring_reservation(RecurringReservation::from_occurrence(
        series_id(series),
        &occurrences[0],
        Some(RecurrenceRule::new(
            RecurrenceFrequency::Weekly,
            1,
            vec![],
            RecurrenceEnd::Never,
        )),
    ));
    occurrences
}

async fn watching(
    repository: &Arc<MockUsageRepository>,
) -> (
    NotifyFutureResourceUsageChangesUseCase<MockUsageRepository, RecordingNotifier>,
    RecordingNotifier,
) {
    let notifier = RecordingNotifier::default();
    let usecase = NotifyFutureResourceUsageChangesUseCase::new(
        repository.clone(),
        notifier.clone(),
        Duration::days(WATCH_WINDOW_DAYS),
    )
    .await
    .unwrap();
    (usecase, notifier)
}

#[tokio::test]
async fn a_new_recurring_reservation_is_announced_once_however_many_occurrences_are_visible() {
    let repository = Arc::new(MockUsageRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    put_weekly_series(&repository, "weekly", &[1, 8, 15, 22, 29, 36, 43, 50, 57]).await;
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "回の数だけ通知してはいけない: {events:?}");
    let NotificationEvent::RecurringReservationCreated(reservation) = &events[0] else {
        panic!("定期予約の作成として伝えるべき: {events:?}");
    };
    assert_eq!(reservation.id(), &series_id("weekly"));
    assert_eq!(
        reservation.rule().map(|rule| rule.frequency()),
        Some(RecurrenceFrequency::Weekly),
        "繰り返しの間隔を添えて伝える"
    );
}

#[tokio::test]
async fn a_recurring_reservation_that_already_existed_at_startup_is_not_announced() {
    let repository = Arc::new(MockUsageRepository::new());
    put_weekly_series(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    usecase.poll_once().await.unwrap();

    assert!(notifier.take().is_empty());
}

#[tokio::test]
async fn an_occurrence_that_slides_into_the_watch_window_is_not_announced() {
    let repository = Arc::new(MockUsageRepository::new());
    put_weekly_series(&repository, "weekly", &[1, 8]).await;
    let notifier = RecordingNotifier::default();
    // 監視範囲を10日に絞り、次の回がまだ範囲の外にある状態から始める
    let usecase = NotifyFutureResourceUsageChangesUseCase::new(
        repository.clone(),
        notifier.clone(),
        Duration::days(10),
    )
    .await
    .unwrap();

    // 時間が経ち、範囲の外にあった回が見えるようになったことを、前回の範囲を
    // 狭く記録し直すことで再現する
    repository.save(&occurrence("weekly", 9)).await.unwrap();
    usecase.previous_state.lock().await.window_end = Utc::now() + Duration::days(8);
    usecase.poll_once().await.unwrap();

    assert!(
        notifier.take().is_empty(),
        "既に伝えた定期予約の次の回が来ただけで、新しい予約ではない"
    );
}

#[tokio::test]
async fn cancelling_a_recurring_reservation_is_announced_once() {
    let repository = Arc::new(MockUsageRepository::new());
    let occurrences = put_weekly_series(&repository, "weekly", &[1, 8, 15, 22]).await;
    let (usecase, notifier) = watching(&repository).await;

    for occurrence in &occurrences {
        repository.delete(occurrence.id()).await.unwrap();
    }
    repository.remove_recurring_reservation(&series_id("weekly"));
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "回の数だけ通知してはいけない: {events:?}");
    let NotificationEvent::RecurringReservationCancelled(reservation) = &events[0] else {
        panic!("定期予約の取り消しとして伝えるべき: {events:?}");
    };
    assert_eq!(
        reservation.rule().map(|rule| rule.frequency()),
        Some(RecurrenceFrequency::Weekly),
        "取り消された定期予約がどういうものだったかを伝えられる"
    );

    usecase.poll_once().await.unwrap();
    assert!(notifier.take().is_empty(), "同じ取り消しを繰り返し伝えない");
}

#[tokio::test]
async fn cancelling_one_occurrence_is_announced_as_that_occurrence() {
    let repository = Arc::new(MockUsageRepository::new());
    let occurrences = put_weekly_series(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    repository.delete(occurrences[1].id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        matches!(
            &events[0],
            NotificationEvent::ResourceUsageDeleted(deleted) if deleted.id() == occurrences[1].id()
        ),
        "定期予約は続いているので、取り消された回だけを伝える: {events:?}"
    );
}

#[tokio::test]
async fn ending_one_occurrence_early_is_announced_as_that_occurrence() {
    let repository = Arc::new(MockUsageRepository::new());
    // 1時間前に始まった回が進行中
    let running =
        usage("weekly-now", Utc::now() - Duration::hours(1)).as_occurrence_of(series_id("weekly"));
    repository.save(&running).await.unwrap();
    put_weekly_series(&repository, "weekly", &[7, 14]).await;
    let (usecase, notifier) = watching(&repository).await;

    let mut released = running.clone();
    released.release_early(Utc::now()).unwrap();
    repository.save(&released).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        matches!(
            &events[0],
            NotificationEvent::ResourceUsageUpdated(updated)
                if updated.id() == running.id()
                    && updated.time_period().end() == released.time_period().end()
        ),
        "締めたのはこの回だけで、取り消しでもない: {events:?}"
    );
}

#[tokio::test]
async fn ending_a_single_reservation_early_is_not_announced_as_a_deletion() {
    let repository = Arc::new(MockUsageRepository::new());
    let running = usage("single", Utc::now() - Duration::hours(1));
    repository.save(&running).await.unwrap();
    let (usecase, notifier) = watching(&repository).await;

    let mut released = running.clone();
    released.release_early(Utc::now()).unwrap();
    repository.save(&released).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::ResourceUsageUpdated(updated)]
                if updated.time_period().end() == released.time_period().end()
        ),
        "使った分は予約として残っている: {events:?}"
    );

    usecase.poll_once().await.unwrap();
    assert!(
        notifier.take().is_empty(),
        "終わった予約について重ねて伝えない"
    );
}

#[tokio::test]
async fn a_change_reaching_several_occurrences_is_announced_as_one_change() {
    let repository = Arc::new(MockUsageRepository::new());
    let occurrences = put_weekly_series(&repository, "weekly", &[1, 8, 15, 22]).await;
    let (usecase, notifier) = watching(&repository).await;

    // 「これ以降のすべての予定」を削除して、途中から打ち切った
    for occurrence in &occurrences[1..] {
        repository.delete(occurrence.id()).await.unwrap();
    }
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "回の数だけ通知してはいけない: {events:?}");
    assert!(
        matches!(
            &events[0],
            NotificationEvent::RecurringReservationUpdated(_)
        ),
        "定期予約の変更として伝えるべき: {events:?}"
    );
}

#[tokio::test]
async fn rescheduling_a_recurring_reservation_is_one_change_not_a_cancellation() {
    let repository = Arc::new(MockUsageRepository::new());
    let occurrences = put_weekly_series(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    // 時間帯を変えると、各回は別のIDを持つ回に置き換わる
    for occurrence in &occurrences {
        repository.delete(occurrence.id()).await.unwrap();
    }
    put_weekly_series(&repository, "weekly", &[2, 9, 16]).await;
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        matches!(
            &events[0],
            NotificationEvent::RecurringReservationUpdated(_)
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_recurring_reservation_the_repository_cannot_describe_is_still_announced_once() {
    let repository = Arc::new(MockUsageRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    // 回は見えるが、定期予約そのものは引けない
    let occurrences = [15, 1, 8].map(|days| occurrence("weekly", days));
    for occurrence in &occurrences {
        repository.save(occurrence).await.unwrap();
    }
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "{events:?}");
    let NotificationEvent::RecurringReservationCreated(reservation) = &events[0] else {
        panic!("定期予約の作成として伝えるべき: {events:?}");
    };
    assert!(reservation.rule().is_none(), "間隔は分からないまま伝える");
    assert_eq!(
        reservation.representative_period().start(),
        occurrences[1].time_period().start(),
        "見えている最初の回が時間帯を代表する"
    );
}

#[tokio::test]
async fn single_reservations_are_still_announced_one_by_one() {
    let repository = Arc::new(MockUsageRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    let first = usage("single-1", Utc::now() + Duration::days(1));
    let second = usage("single-2", Utc::now() + Duration::days(2));
    repository.save(&first).await.unwrap();
    repository.save(&second).await.unwrap();
    usecase.poll_once().await.unwrap();

    let created = notifier.take();
    assert_eq!(created.len(), 2, "{created:?}");
    assert!(
        created
            .iter()
            .all(|event| matches!(event, NotificationEvent::ResourceUsageCreated(_)))
    );

    repository.delete(first.id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let deleted = notifier.take();
    assert!(
        matches!(
            deleted.as_slice(),
            [NotificationEvent::ResourceUsageDeleted(usage)] if usage.id() == first.id()
        ),
        "{deleted:?}"
    );
}

#[tokio::test]
async fn a_reservation_that_ran_its_course_is_not_announced() {
    let repository = Arc::new(MockUsageRepository::new());
    // 既に終わりかけの予約。次のポーリングまでに予定どおり終わる
    let ending = ResourceUsage::reconstruct(
        UsageId::from_string("ending".to_string()),
        EmailAddress::new("owner@example.com".to_string()).unwrap(),
        TimePeriod::new(
            Utc::now() - Duration::hours(1),
            Utc::now() + Duration::milliseconds(50),
        )
        .unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        None,
    )
    .unwrap();
    repository.save(&ending).await.unwrap();
    let (usecase, notifier) = watching(&repository).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    usecase.poll_once().await.unwrap();

    assert!(notifier.take().is_empty());
}
