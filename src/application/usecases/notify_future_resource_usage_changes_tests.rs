//! `NotifyFutureResourceUsageChangesUseCase`のテスト

use super::*;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, UsageId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
use async_trait::async_trait;
use chrono::DateTime;
use std::sync::Mutex;

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
        Duration::days(60),
    )
    .await
    .unwrap();
    (usecase, notifier)
}

#[tokio::test]
async fn ending_a_reservation_early_is_not_announced_as_a_deletion() {
    let repository = Arc::new(MockUsageRepository::new());
    let running = usage("running", Utc::now() - Duration::hours(1));
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
async fn cancelling_a_reservation_is_still_announced_as_a_deletion() {
    let repository = Arc::new(MockUsageRepository::new());
    let upcoming = usage("upcoming", Utc::now() + Duration::days(1));
    repository.save(&upcoming).await.unwrap();
    let (usecase, notifier) = watching(&repository).await;

    repository.delete(upcoming.id()).await.unwrap();
    usecase.poll_once().await.unwrap();

    let events = notifier.take();
    assert!(
        matches!(
            events.as_slice(),
            [NotificationEvent::ResourceUsageDeleted(deleted)] if deleted.id() == upcoming.id()
        ),
        "{events:?}"
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
