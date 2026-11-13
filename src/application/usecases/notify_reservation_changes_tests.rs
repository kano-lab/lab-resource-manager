//! `NotifyReservationChangesUseCase`のテスト
//!
//! 保存先の予約枠の変化が、予約に起きた出来事として予約チャンネルへ伝わることを検証する。
//! 変化の読み取りそのものは`ReservationChangeInterpreter`のテストが詳しく扱う。

use super::*;
use crate::application::clock::FixedClock;
use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
use crate::domain::aggregates::reservation::{
    OccurrenceKey, RecurrenceRule, RecurringSchedule, ReservationEvent, ReservedSlotId, Schedule,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::read_models::ReservedSlotView;
use crate::infrastructure::repositories::reservation::in_memory::InMemoryReservationRepository;
use async_trait::async_trait;
use chrono::TimeZone;
use std::collections::BTreeMap;
use std::sync::Mutex;

const WATCH_WINDOW_DAYS: i64 = 60;

/// テストの「今」
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap()
}

/// 受け取った通知を記録するテストダブル
///
/// `fail_once_for`の予約についての通知は、最初の1回だけ送れなかったことにする。
#[derive(Clone, Default)]
struct RecordingNotifier {
    events: Arc<Mutex<Vec<ReservationEvent>>>,
    fail_once_for: Arc<Mutex<Option<String>>>,
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
        let mut fail_once_for = self.fail_once_for.lock().unwrap();
        if fail_once_for.as_deref() == Some(event.reservation().id().as_str()) {
            *fail_once_for = None;
            return Err(NotificationError::SendFailure(
                "Slackが応答しない".to_string(),
            ));
        }
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

/// 単発予約の予約枠
fn slot(id: &str, start: DateTime<Utc>) -> ReservedSlotView {
    ReservedSlotView::reconstruct(
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

/// 今（`now()`）から監視を始める。時計を進めればポーリングの時刻も進む
async fn watching(
    repository: &Arc<InMemoryReservationRepository>,
) -> (
    NotifyReservationChangesUseCase<InMemoryReservationRepository, RecordingNotifier>,
    RecordingNotifier,
    Arc<FixedClock>,
) {
    let notifier = RecordingNotifier::default();
    let clock = Arc::new(FixedClock::at(now()));
    let usecase = NotifyReservationChangesUseCase::with_clock(
        repository.clone(),
        notifier.clone(),
        Duration::days(WATCH_WINDOW_DAYS),
        clock.clone(),
    )
    .await
    .unwrap();
    (usecase, notifier, clock)
}

#[tokio::test]
async fn one_time_reservations_are_announced_one_by_one() {
    let repository = Arc::new(InMemoryReservationRepository::new());
    let (usecase, notifier, _clock) = watching(&repository).await;

    let first = slot("one-time-1", now() + Duration::days(1));
    let second = slot("one-time-2", now() + Duration::days(2));
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

    repository.delete_reservation(first.reservation_id());
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
    let repository = Arc::new(InMemoryReservationRepository::new());
    let running = slot("one-time", now() - Duration::hours(1));
    repository.put_slot(&running);
    let (usecase, notifier, _clock) = watching(&repository).await;

    let mut ended = repository
        .find(running.reservation_id())
        .await
        .unwrap()
        .unwrap();
    ended
        .end_slot_early(&OccurrenceKey::Only, now(), running.holder_email())
        .unwrap();
    repository.save(&mut ended).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [ReservationEvent::EndedEarly(reservation)]
                if reservation.schedule() == ended.schedule()
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
    let repository = Arc::new(InMemoryReservationRepository::new());
    // 1分後に予定どおり終わる予約枠
    let ending = ReservedSlotView::reconstruct(
        ReservedSlotId::from_string("ending".to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
        TimePeriod::new(now() - Duration::hours(1), now() + Duration::minutes(1)).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        None,
    )
    .unwrap();
    repository.put_slot(&ending);
    let (usecase, notifier, clock) = watching(&repository).await;

    clock.advance(Duration::minutes(2));
    usecase.poll_once().await.unwrap();

    assert!(
        notifier.take().is_empty(),
        "予定どおり終わって見えなくなった予約枠は、取り消しではない"
    );
}

#[tokio::test]
async fn an_announcement_that_failed_is_sent_on_the_next_pass_and_the_others_are_not_repeated() {
    let repository = Arc::new(InMemoryReservationRepository::new());
    let (usecase, notifier, _clock) = watching(&repository).await;
    let first = slot("first", now() + Duration::days(1));
    let second = slot("second", now() + Duration::days(2));
    let third = slot("third", now() + Duration::days(3));
    for each in [&first, &second, &third] {
        repository.put_slot(each);
    }
    *notifier.fail_once_for.lock().unwrap() = Some("second".to_string());

    let failed = usecase.poll_once().await;
    let sent_first: Vec<String> = notifier
        .take()
        .iter()
        .map(|event| event.reservation().id().as_str().to_string())
        .collect();
    usecase.poll_once().await.unwrap();
    let sent_next = notifier.take();

    assert!(
        matches!(failed, Err(ApplicationError::Notification(_))),
        "{failed:?}"
    );
    let mut sent_first = sent_first;
    sent_first.sort();
    assert_eq!(
        sent_first,
        vec!["first", "third"],
        "送れなかった1件のほかは送る"
    );
    assert!(
        matches!(
            sent_next.as_slice(),
            [ReservationEvent::Reserved(reservation)] if reservation.id().as_str() == "second"
        ),
        "次の回で、送れなかった1件だけを送り直す: {sent_next:?}"
    );
}

#[tokio::test]
async fn cancelling_one_occurrence_of_a_recurring_reservation_is_announced_as_that_slot() {
    let repository = Arc::new(InMemoryReservationRepository::new());
    let first_start = now() + Duration::days(1);
    let weekly = Reservation::reconstruct(
        ReservationId::from_string("weekly".to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        None,
        Schedule::Recurring(RecurringSchedule::new(
            TimePeriod::new(first_start, first_start + Duration::hours(2)).unwrap(),
            RecurrenceRule::parse(vec!["RRULE:FREQ=WEEKLY;COUNT=4".to_string()]).unwrap(),
            chrono_tz::Asia::Tokyo,
            BTreeMap::new(),
        )),
        None,
    )
    .unwrap();
    repository.put_reservation(&weekly);
    let (usecase, notifier, _clock) = watching(&repository).await;

    let second = OccurrenceKey::At(first_start + Duration::days(7));
    let mut stored = repository.find(weekly.id()).await.unwrap().unwrap();
    stored.cancel_slot(&second, weekly.holder_email()).unwrap();
    repository.save(&mut stored).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [ReservationEvent::SlotCancelled { slot, .. }] if slot.key() == &second
        ),
        "定期予約の取り消しではなく、その回の取り消しとして伝える: {events:?}"
    );
}

#[tokio::test]
async fn extending_one_occurrence_of_a_recurring_reservation_is_announced_as_that_slot() {
    let repository = Arc::new(InMemoryReservationRepository::new());
    let first_start = now() + Duration::days(1);
    let weekly = crate::application::usecases::test_support::weekly_reservation(first_start);
    repository.put_reservation(&weekly);
    let (usecase, notifier, _clock) = watching(&repository).await;

    // 2回目だけ、2時間を3時間に伸ばす
    let second_start = first_start + Duration::days(7);
    let second = OccurrenceKey::At(second_start);
    let longer = TimePeriod::new(second_start, second_start + Duration::hours(3)).unwrap();
    let mut stored = repository.find(weekly.id()).await.unwrap().unwrap();
    stored
        .reschedule_slot(&second, longer.clone(), weekly.holder_email())
        .unwrap();
    repository.save(&mut stored).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [ReservationEvent::SlotChanged { slot, .. }]
                if slot.key() == &second && slot.time_period() == &longer
        ),
        "伸ばした回の予約枠の変更として、1件だけ伝える: {events:?}"
    );
}

/// 毎日、`first_start`から2時間の定期予約（ID・規則を指定）
fn daily(id: &str, first_start: DateTime<Utc>, rule: &str) -> Reservation {
    Reservation::reconstruct(
        ReservationId::from_string(id.to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        Some("毎日の学習".to_string()),
        Schedule::Recurring(RecurringSchedule::new(
            TimePeriod::new(first_start, first_start + Duration::hours(2)).unwrap(),
            RecurrenceRule::parse(vec![rule.to_string()]).unwrap(),
            chrono_tz::Asia::Tokyo,
            BTreeMap::new(),
        )),
        None,
    )
    .unwrap()
}

/// `until`までで打ち切る規則
fn until(rule: &str, until: DateTime<Utc>) -> String {
    format!("{rule};UNTIL={}", until.format("%Y%m%dT%H%M%SZ"))
}

#[tokio::test]
async fn changing_a_recurring_reservation_from_one_occurrence_on_is_two_announcements_not_one_per_slot()
 {
    // 毎日の定期予約。監視している60日には予約枠が60個ある
    let repository = Arc::new(InMemoryReservationRepository::new());
    let first = now() + Duration::days(1);
    repository.put_reservation(&daily("series", first, "RRULE:FREQ=DAILY"));
    let (usecase, notifier, _clock) = watching(&repository).await;

    // カレンダーで10回目の予定を開き、「これ以降のすべての予定」の時間を1時間遅らせた。
    // カレンダーは元の繰り返しを9回目までで打ち切り、10回目から新しい繰り返しを作る
    let tenth = first + Duration::days(9);
    repository.put_reservation(&daily(
        "series",
        first,
        &until("RRULE:FREQ=DAILY", tenth - Duration::seconds(1)),
    ));
    repository.put_reservation(&daily(
        "series_R20261126T010000",
        tenth + Duration::hours(1),
        "RRULE:FREQ=DAILY",
    ));
    usecase.poll_once().await.unwrap();

    let mut events: Vec<String> = notifier
        .take()
        .iter()
        .map(|event| {
            let kind = match event {
                ReservationEvent::Reserved(_) => "reserved",
                ReservationEvent::Changed(_) => "changed",
                other => panic!("予約全体の出来事のはず: {other:?}"),
            };
            format!("{kind} {}", event.reservation().id().as_str())
        })
        .collect();
    events.sort();
    assert_eq!(
        events,
        vec!["changed series", "reserved series_R20261126T010000"],
        "打ち切った元の予約の変更と、新しい予約の2件だけ。予約枠ごとには伝えない"
    );
}

#[tokio::test]
async fn cancelling_a_recurring_reservation_from_one_occurrence_on_is_one_announcement() {
    // カレンダーの「これ以降のすべての予定を削除」：元の繰り返しを打ち切るだけ
    let repository = Arc::new(InMemoryReservationRepository::new());
    let first = now() + Duration::days(1);
    repository.put_reservation(&daily("series", first, "RRULE:FREQ=DAILY"));
    let (usecase, notifier, _clock) = watching(&repository).await;

    let tenth = first + Duration::days(9);
    repository.put_reservation(&daily(
        "series",
        first,
        &until("RRULE:FREQ=DAILY", tenth - Duration::seconds(1)),
    ));
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(events.as_slice(), [ReservationEvent::Changed(reservation)] if reservation.id().as_str() == "series"),
        "取り消した51個の予約枠ごとではなく、1件: {events:?}"
    );
}
