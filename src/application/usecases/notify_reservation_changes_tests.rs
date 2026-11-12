//! `NotifyReservationChangesUseCase`のテスト
//!
//! 通知が予約を単位とし、定期予約が予約枠の数だけの通知にならないことを検証する。

use super::*;
use crate::domain::aggregates::reservation::{
    RecurrenceEnd, RecurrenceFrequency, RecurrenceRule,
    value_objects::{Gpu, Resource},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
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

fn reservation_id(id: &str) -> ReservationId {
    ReservationId::from_string(id.to_string())
}

/// 単発予約の予約枠
fn slot(id: &str, start: DateTime<Utc>) -> ReservedSlot {
    ReservedSlot::reconstruct(
        ReservedSlotId::from_string(id.to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
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

/// 定期予約`reservation`の、今から`days_ahead`日後の予約枠
fn recurring_slot(reservation: &str, days_ahead: i64) -> ReservedSlot {
    slot(
        &format!("{reservation}-{days_ahead}"),
        Utc::now() + Duration::days(days_ahead),
    )
    .of_recurring_reservation(reservation_id(reservation))
}

/// 毎週の定期予約を、監視範囲に入る予約枠とともに登録する
async fn put_weekly(
    repository: &MockReservationRepository,
    reservation: &str,
    days_ahead: &[i64],
) -> Vec<ReservedSlot> {
    let slots: Vec<ReservedSlot> = days_ahead
        .iter()
        .map(|days| recurring_slot(reservation, *days))
        .collect();
    for slot in &slots {
        repository.save_slot(slot).await.unwrap();
    }
    repository.put_recurring_reservation(RecurringReservation::from_first_slot(
        reservation_id(reservation),
        &slots[0],
        Some(RecurrenceRule::new(
            RecurrenceFrequency::Weekly,
            1,
            vec![],
            RecurrenceEnd::Never,
        )),
    ));
    slots
}

async fn watching(
    repository: &Arc<MockReservationRepository>,
) -> (
    NotifyReservationChangesUseCase<MockReservationRepository, RecordingNotifier>,
    RecordingNotifier,
) {
    let notifier = RecordingNotifier::default();
    let usecase = NotifyReservationChangesUseCase::new(
        repository.clone(),
        notifier.clone(),
        Duration::days(WATCH_WINDOW_DAYS),
    )
    .await
    .unwrap();
    (usecase, notifier)
}

#[tokio::test]
async fn a_new_recurring_reservation_is_announced_once_however_many_slots_are_visible() {
    let repository = Arc::new(MockReservationRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    put_weekly(&repository, "weekly", &[1, 8, 15, 22, 29, 36, 43, 50, 57]).await;
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(
        events.len(),
        1,
        "予約枠の数だけ通知してはいけない: {events:?}"
    );
    let NotificationEvent::ReservationCreated(Reservation::Recurring(reservation)) = &events[0]
    else {
        panic!("定期予約の作成として伝えるべき: {events:?}");
    };
    assert_eq!(reservation.id(), &reservation_id("weekly"));
    assert_eq!(
        reservation.rule().map(|rule| rule.frequency()),
        Some(RecurrenceFrequency::Weekly),
        "繰り返しの規則を添えて伝える"
    );
}

#[tokio::test]
async fn a_recurring_reservation_that_already_existed_at_startup_is_not_announced() {
    let repository = Arc::new(MockReservationRepository::new());
    put_weekly(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    usecase.poll_once().await.unwrap();

    assert!(notifier.take().is_empty());
}

#[tokio::test]
async fn a_slot_that_slides_into_the_watch_window_is_not_announced() {
    let repository = Arc::new(MockReservationRepository::new());
    put_weekly(&repository, "weekly", &[1, 8]).await;
    let notifier = RecordingNotifier::default();
    // 監視範囲を10日に絞り、次の予約枠がまだ範囲の外にある状態から始める
    let usecase = NotifyReservationChangesUseCase::new(
        repository.clone(),
        notifier.clone(),
        Duration::days(10),
    )
    .await
    .unwrap();

    // 時間が経ち、範囲の外にあった予約枠が見えるようになったことを、前回の範囲を
    // 狭く記録し直すことで再現する
    repository
        .save_slot(&recurring_slot("weekly", 9))
        .await
        .unwrap();
    usecase.previous_state.lock().await.window_end = Utc::now() + Duration::days(8);
    usecase.poll_once().await.unwrap();

    assert!(
        notifier.take().is_empty(),
        "既に伝えた定期予約の次の予約枠が来ただけで、予約は増えていない"
    );
}

#[tokio::test]
async fn cancelling_a_recurring_reservation_is_announced_once() {
    let repository = Arc::new(MockReservationRepository::new());
    put_weekly(&repository, "weekly", &[1, 8, 15, 22]).await;
    let (usecase, notifier) = watching(&repository).await;

    repository
        .delete_reservation(&reservation_id("weekly"))
        .await
        .unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(
        events.len(),
        1,
        "予約枠の数だけ通知してはいけない: {events:?}"
    );
    let NotificationEvent::ReservationCancelled(Reservation::Recurring(reservation)) = &events[0]
    else {
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
async fn cancelling_one_slot_is_announced_as_that_slot() {
    let repository = Arc::new(MockReservationRepository::new());
    let slots = put_weekly(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    repository.delete_slot(slots[1].id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::SlotCancelled(cancelled)] if cancelled.id() == slots[1].id()
        ),
        "定期予約は続いているので、取り消された予約枠だけを伝える: {events:?}"
    );
}

#[tokio::test]
async fn ending_one_slot_early_is_announced_as_that_slot() {
    let repository = Arc::new(MockReservationRepository::new());
    // 1時間前に始まった予約枠が進行中
    let running = slot("weekly-now", Utc::now() - Duration::hours(1))
        .of_recurring_reservation(reservation_id("weekly"));
    repository.save_slot(&running).await.unwrap();
    put_weekly(&repository, "weekly", &[7, 14]).await;
    let (usecase, notifier) = watching(&repository).await;

    let mut ended = running.clone();
    ended.end_early(Utc::now()).unwrap();
    repository.save_slot(&ended).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::SlotUpdated(updated)]
                if updated.id() == running.id()
                    && updated.time_period().end() == ended.time_period().end()
        ),
        "締めたのはこの予約枠だけで、取り消しでもない: {events:?}"
    );
}

#[tokio::test]
async fn a_change_reaching_several_slots_is_announced_as_one_change() {
    let repository = Arc::new(MockReservationRepository::new());
    let slots = put_weekly(&repository, "weekly", &[1, 8, 15, 22]).await;
    let (usecase, notifier) = watching(&repository).await;

    // 「これ以降のすべての予定」を削除して、途中から打ち切った
    for slot in &slots[1..] {
        repository.delete_slot(slot.id()).await.unwrap();
    }
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::ReservationUpdated(
                Reservation::Recurring(_)
            )]
        ),
        "定期予約の変更として1件で伝えるべき: {events:?}"
    );
}

#[tokio::test]
async fn rescheduling_a_recurring_reservation_is_one_change_not_a_cancellation() {
    let repository = Arc::new(MockReservationRepository::new());
    let slots = put_weekly(&repository, "weekly", &[1, 8, 15]).await;
    let (usecase, notifier) = watching(&repository).await;

    // 時間帯を変えると、各予約枠は別のIDを持つ予約枠に置き換わる
    for slot in &slots {
        repository.delete_slot(slot.id()).await.unwrap();
    }
    put_weekly(&repository, "weekly", &[2, 9, 16]).await;
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::ReservationUpdated(
                Reservation::Recurring(_)
            )]
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_recurring_reservation_the_repository_cannot_describe_is_still_announced_once() {
    let repository = Arc::new(MockReservationRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    // 予約枠は見えるが、定期予約そのものは引けない
    let slots = [15, 1, 8].map(|days| recurring_slot("weekly", days));
    for slot in &slots {
        repository.save_slot(slot).await.unwrap();
    }
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert_eq!(events.len(), 1, "{events:?}");
    let NotificationEvent::ReservationCreated(Reservation::Recurring(reservation)) = &events[0]
    else {
        panic!("定期予約の作成として伝えるべき: {events:?}");
    };
    assert!(
        reservation.rule().is_none(),
        "繰り返しの規則は分からないまま伝える"
    );
    assert_eq!(
        reservation.first_slot_period().start(),
        slots[1].time_period().start(),
        "見えている最も早い予約枠が時間帯を決める"
    );
}

#[tokio::test]
async fn one_time_reservations_are_announced_one_by_one() {
    let repository = Arc::new(MockReservationRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    let first = slot("one-time-1", Utc::now() + Duration::days(1));
    let second = slot("one-time-2", Utc::now() + Duration::days(2));
    repository.save_slot(&first).await.unwrap();
    repository.save_slot(&second).await.unwrap();
    usecase.poll_once().await.unwrap();

    let created = notifier.take();
    assert_eq!(created.len(), 2, "{created:?}");
    assert!(created.iter().all(|event| matches!(
        event,
        NotificationEvent::ReservationCreated(Reservation::OneTime(_))
    )));

    repository.delete_slot(first.id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let cancelled = notifier.take();
    assert!(
        matches!(
            cancelled.as_slice(),
            [NotificationEvent::ReservationCancelled(Reservation::OneTime(reservation))]
                if reservation.id().as_str() == first.id().as_str()
        ),
        "{cancelled:?}"
    );
}

#[tokio::test]
async fn ending_a_one_time_reservation_early_is_not_announced_as_a_cancellation() {
    let repository = Arc::new(MockReservationRepository::new());
    let running = slot("one-time", Utc::now() - Duration::hours(1));
    repository.save_slot(&running).await.unwrap();
    let (usecase, notifier) = watching(&repository).await;

    let mut ended = running.clone();
    ended.end_early(Utc::now()).unwrap();
    repository.save_slot(&ended).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::ReservationUpdated(Reservation::OneTime(reservation))]
                if reservation.time_period().end() == ended.time_period().end()
        ),
        "使った分は予約枠として残っている: {events:?}"
    );

    usecase.poll_once().await.unwrap();
    assert!(
        notifier.take().is_empty(),
        "終わった予約について重ねて伝えない"
    );
}

#[tokio::test]
async fn a_slot_that_ran_its_course_is_not_announced() {
    let repository = Arc::new(MockReservationRepository::new());
    // 既に終わりかけの予約枠。次のポーリングまでに予定どおり終わる
    let ending = ReservedSlot::reconstruct(
        ReservedSlotId::from_string("ending".to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
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
    repository.save_slot(&ending).await.unwrap();
    let (usecase, notifier) = watching(&repository).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    usecase.poll_once().await.unwrap();

    assert!(notifier.take().is_empty());
}
