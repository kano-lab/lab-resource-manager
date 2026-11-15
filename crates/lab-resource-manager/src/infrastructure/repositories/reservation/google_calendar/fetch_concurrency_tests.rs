//! カレンダーの読み込みを同時に行うことのテスト
//!
//! Slackのコマンドは3秒以内に応えなければならない。カレンダーを1つずつ読むと、
//! 応答時間がカレンダーの数に比例して伸びてこの期限を超える。

use super::event_gateway::CalendarEventGateway;
use super::repository::GoogleCalendarReservationRepository;
use super::repository_tests::{
    FakeCalendarEventGateway, SERVICE_ACCOUNT_EMAIL, StubIdentityLinkRepository,
};
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::infrastructure::config::{
    DeviceConfig, ResourceConfig, ServerConfig, StorageBackend, StorageConfig,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use google_calendar3::api::Event;
use std::collections::HashMap;
use std::sync::Arc;

/// 1回の一覧取得にかかる時間
const LIST_LATENCY: std::time::Duration = std::time::Duration::from_secs(1);

/// カレンダーの数
const CALENDAR_COUNT: usize = 5;

/// 一覧の取得だけに時間がかかる、偽のカレンダー
struct SlowCalendarEventGateway {
    inner: FakeCalendarEventGateway,
}

#[async_trait]
impl CalendarEventGateway for SlowCalendarEventGateway {
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError> {
        tokio::time::sleep(LIST_LATENCY).await;
        self.inner
            .list_events(calendar_id, time_min, time_max)
            .await
    }

    async fn list_series(
        &self,
        calendar_id: &str,
        ical_uid: &str,
    ) -> Result<Vec<Event>, RepositoryError> {
        self.inner.list_series(calendar_id, ical_uid).await
    }

    async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        self.inner.get_event(calendar_id, event_id).await
    }

    async fn insert_event(
        &self,
        calendar_id: &str,
        event: Event,
    ) -> Result<Event, RepositoryError> {
        self.inner.insert_event(calendar_id, event).await
    }

    async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: Event,
        version: &str,
    ) -> Result<Event, RepositoryError> {
        self.inner
            .update_event(calendar_id, event_id, event, version)
            .await
    }

    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        version: &str,
    ) -> Result<(), RepositoryError> {
        self.inner
            .delete_event(calendar_id, event_id, version)
            .await
    }
}

fn server_name(index: usize) -> String {
    format!("server-{index}")
}

/// サーバーごとにカレンダーを持つリポジトリ
fn repository_with_many_calendars() -> GoogleCalendarReservationRepository {
    let servers = (0..CALENDAR_COUNT)
        .map(|index| ServerConfig {
            name: server_name(index),
            devices: vec![DeviceConfig {
                id: 0,
                model: "A100 80GB PCIe".to_string(),
            }],
            notifications: vec![],
        })
        .collect();
    let calendars: HashMap<String, String> = (0..CALENDAR_COUNT)
        .map(|index| (server_name(index), format!("calendar-{index}")))
        .collect();

    GoogleCalendarReservationRepository::with_gateway(
        Arc::new(SlowCalendarEventGateway {
            inner: FakeCalendarEventGateway::new(vec![]),
        }),
        ResourceConfig {
            servers,
            rooms: vec![],
        },
        SERVICE_ACCOUNT_EMAIL.to_string(),
        Arc::new(StubIdentityLinkRepository::default()),
        StorageConfig {
            backend: StorageBackend::GoogleCalendar { calendars },
        },
    )
    .expect("テスト用リポジトリの構築に失敗")
}

#[tokio::test(start_paused = true)]
async fn calendars_are_read_at_the_same_time() {
    let repository = repository_with_many_calendars();
    let now = Utc::now();
    let period = TimePeriod::new(now, now + Duration::days(7)).unwrap();

    let started = tokio::time::Instant::now();
    repository.find_overlapping(&period).await.unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed < LIST_LATENCY * 2,
        "{CALENDAR_COUNT}個のカレンダーの読み込みに{elapsed:?}かかった。1つずつ読んでいる"
    );
}
