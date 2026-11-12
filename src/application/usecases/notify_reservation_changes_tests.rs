//! `NotifyReservationChangesUseCase`のテスト
//!
//! 保存先の予約枠の変化が、予約に起きた出来事として予約チャンネルへ伝わることを検証する。
//! 変化の読み取りそのものは`ReservationChangeInterpreter`のテストが詳しく扱う。

use super::*;
use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
use crate::domain::aggregates::reservation::{ReservationEvent, Schedule};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
use async_trait::async_trait;
use chrono::DateTime;
use std::sync::Mutex;

const WATCH_WINDOW_DAYS: i64 = 60;

/// 受け取った通知を記録するテストダブル
#[derive(Clone, Default)]
struct RecordingNotifier {
    events: Arc<Mutex<Vec<ReservationEvent>>>,
}

impl RecordingNotifier {
    /// これまでに受け取った通知を取り出し、記録を空にする
    fn take(&self) -> Vec<ReservationEvent> {
        std::mem::take(&mut self.events.lock().unwrap())
    }
}

#[async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, event: ReservationEvent) -> Result<(), NotificationError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
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
async fn one_time_reservations_are_announced_one_by_one() {
    let repository = Arc::new(MockReservationRepository::new());
    let (usecase, notifier) = watching(&repository).await;

    let first = slot("one-time-1", Utc::now() + Duration::days(1));
    let second = slot("one-time-2", Utc::now() + Duration::days(2));
    repository.put_slot(&first);
    repository.put_slot(&second);
    usecase.poll_once().await.unwrap();

    let created = notifier.take();
    assert_eq!(created.len(), 2, "{created:?}");
    assert!(
        created
            .iter()
            .all(|event| matches!(event, ReservationEvent::Reserved(_)))
    );

    repository.remove_slot(first.id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let cancelled = notifier.take();
    assert!(
        matches!(
            cancelled.as_slice(),
            [ReservationEvent::Cancelled(reservation)]
                if reservation.id().as_str() == first.id().as_str()
        ),
        "{cancelled:?}"
    );
}

#[tokio::test]
async fn ending_a_one_time_reservation_early_is_not_announced_as_a_cancellation() {
    let repository = Arc::new(MockReservationRepository::new());
    let running = slot("one-time", Utc::now() - Duration::hours(1));
    repository.put_slot(&running);
    let (usecase, notifier) = watching(&repository).await;

    let mut ended = running.clone();
    ended.end_early(Utc::now(), running.holder_email()).unwrap();
    repository.save_slot(&ended).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [ReservationEvent::EndedEarly(reservation)]
                if reservation.schedule() == &Schedule::OneTime(ended.time_period().clone())
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
    repository.put_slot(&ending);
    let (usecase, notifier) = watching(&repository).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    usecase.poll_once().await.unwrap();

    assert!(notifier.take().is_empty());
}
