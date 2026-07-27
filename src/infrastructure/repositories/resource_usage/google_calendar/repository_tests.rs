//! `GoogleCalendarUsageRepository`の検索系メソッドのテスト
//!
//! カレンダーAPIへのアクセスは`CalendarEventGateway`のテストダブルに差し替え、
//! 「どの期間を問い合わせるか」「取得したイベントをどう解釈するか」を検証する。

use super::event_gateway::CalendarEventGateway;
use super::repository::GoogleCalendarUsageRepository;
use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use crate::infrastructure::config::{DeviceConfig, ResourceConfig, ServerConfig};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use google_calendar3::api::{Event, EventCreator, EventDateTime};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SERVICE_ACCOUNT_EMAIL: &str = "service-account@example.com";
const CALENDAR_ID: &str = "thalys-calendar-id";

/// ゲートウェイが受け取った問い合わせ内容
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListQuery {
    calendar_id: String,
    time_min: DateTime<Utc>,
    time_max: Option<DateTime<Utc>>,
}

/// 固定のイベント集合を返し、受け取った問い合わせを記録するテストダブル
///
/// `list_events`はGoogle Calendar APIの絞り込み意味論を模倣する。
/// すなわち終了時刻が`time_min`より後、かつ（`time_max`指定時は）開始時刻が
/// `time_max`より前のイベントだけを返す。
struct FakeCalendarEventGateway {
    events: Vec<Event>,
    queries: Mutex<Vec<ListQuery>>,
}

impl FakeCalendarEventGateway {
    fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            queries: Mutex::new(Vec::new()),
        }
    }

    fn queries(&self) -> Vec<ListQuery> {
        self.queries.lock().unwrap().clone()
    }
}

fn event_start(event: &Event) -> DateTime<Utc> {
    event
        .start
        .as_ref()
        .and_then(|s| s.date_time)
        .expect("テストイベントには開始時刻が必要")
}

fn event_end(event: &Event) -> DateTime<Utc> {
    event
        .end
        .as_ref()
        .and_then(|e| e.date_time)
        .expect("テストイベントには終了時刻が必要")
}

#[async_trait]
impl CalendarEventGateway for FakeCalendarEventGateway {
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError> {
        self.queries.lock().unwrap().push(ListQuery {
            calendar_id: calendar_id.to_string(),
            time_min,
            time_max,
        });

        Ok(self
            .events
            .iter()
            .filter(|event| event_end(event) > time_min)
            .filter(|event| time_max.is_none_or(|max| event_start(event) < max))
            .cloned()
            .collect())
    }

    async fn get_event(
        &self,
        _calendar_id: &str,
        _event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }

    async fn insert_event(
        &self,
        _calendar_id: &str,
        _event: Event,
    ) -> Result<Event, RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }

    async fn update_event(
        &self,
        _calendar_id: &str,
        _event_id: &str,
        _event: Event,
    ) -> Result<(), RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }

    async fn delete_event(
        &self,
        _calendar_id: &str,
        _event_id: &str,
    ) -> Result<(), RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }
}

/// テスト終了時に自動削除される一時ファイルパス
struct TempMappingPath(PathBuf);

impl TempMappingPath {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("lrm-id-mappings-{}.json", uuid::Uuid::new_v4())))
    }

    fn path(&self) -> PathBuf {
        self.0.clone()
    }
}

impl Drop for TempMappingPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn test_config() -> ResourceConfig {
    ResourceConfig {
        servers: vec![ServerConfig {
            name: "Thalys".to_string(),
            calendar_id: CALENDAR_ID.to_string(),
            devices: vec![
                DeviceConfig {
                    id: 0,
                    model: "A100 80GB PCIe".to_string(),
                },
                DeviceConfig {
                    id: 1,
                    model: "A100 80GB PCIe".to_string(),
                },
            ],
            notifications: vec![],
        }],
        rooms: vec![],
    }
}

/// アプリが作成した予約イベント（予約者はdescriptionのアプリ管理セクションに入る）
fn reservation_event(
    event_id: &str,
    gpu_spec: &str,
    owner_email: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Event {
    Event {
        id: Some(event_id.to_string()),
        summary: Some(gpu_spec.to_string()),
        description: Some(format!(
            "[lab-resource-manager:managed-section:begin]\n予約者: {}\n[lab-resource-manager:managed-section:end]",
            owner_email
        )),
        creator: Some(EventCreator {
            email: Some(SERVICE_ACCOUNT_EMAIL.to_string()),
            ..Default::default()
        }),
        start: Some(EventDateTime {
            date_time: Some(start),
            ..Default::default()
        }),
        end: Some(EventDateTime {
            date_time: Some(end),
            ..Default::default()
        }),
        status: Some("confirmed".to_string()),
        ..Default::default()
    }
}

fn repository_with(
    events: Vec<Event>,
) -> (
    GoogleCalendarUsageRepository,
    Arc<FakeCalendarEventGateway>,
    TempMappingPath,
) {
    let gateway = Arc::new(FakeCalendarEventGateway::new(events));
    let mapping_path = TempMappingPath::new();
    let repository = GoogleCalendarUsageRepository::with_gateway(
        gateway.clone(),
        test_config(),
        SERVICE_ACCOUNT_EMAIL.to_string(),
        mapping_path.path(),
    )
    .expect("テスト用リポジトリの構築に失敗");

    (repository, gateway, mapping_path)
}

#[tokio::test]
async fn find_overlapping_includes_reservation_that_already_ended() {
    let now = Utc::now();
    // 5時間前に始まり3時間前に終わった予約（事後予約の承諾が遅れた場合に相手となるもの）
    let existing = reservation_event(
        "already-ended",
        "0",
        "owner@example.com",
        now - Duration::hours(5),
        now - Duration::hours(3),
    );
    let (repository, _gateway, _mapping_path) = repository_with(vec![existing]);

    // 4時間前〜2時間前を予約しようとすると、上の予約と重なる
    let period = TimePeriod::new(now - Duration::hours(4), now - Duration::hours(2)).unwrap();

    let overlapping = repository.find_overlapping(&period).await.unwrap();

    assert_eq!(
        overlapping.len(),
        1,
        "終了時刻が過去の予約も競合チェックの対象に含めるべき"
    );
}

#[tokio::test]
async fn find_overlapping_queries_exactly_the_requested_period() {
    let now = Utc::now();
    let (repository, gateway, _mapping_path) = repository_with(vec![]);
    let period = TimePeriod::new(now - Duration::hours(4), now - Duration::hours(2)).unwrap();

    repository.find_overlapping(&period).await.unwrap();

    let queries = gateway.queries();
    assert_eq!(
        queries.len(),
        1,
        "サーバーカレンダー1件分の問い合わせが行われる"
    );
    assert_eq!(
        queries[0].time_min,
        period.start(),
        "問い合わせの下限は対象期間の開始時刻であるべき"
    );
    assert_eq!(
        queries[0].time_max,
        Some(period.end()),
        "問い合わせの上限は対象期間の終了時刻であるべき"
    );
}

#[tokio::test]
async fn find_overlapping_ignores_cancelled_events() {
    let now = Utc::now();
    let mut cancelled = reservation_event(
        "cancelled-event",
        "0",
        "owner@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    cancelled.status = Some("cancelled".to_string());
    let (repository, _gateway, _mapping_path) = repository_with(vec![cancelled]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let overlapping = repository.find_overlapping(&period).await.unwrap();

    assert!(
        overlapping.is_empty(),
        "キャンセル済みイベントは予約として扱わない"
    );
}

#[tokio::test]
async fn find_overlapping_skips_unparsable_events_but_keeps_the_rest() {
    let now = Utc::now();
    // GPUデバイス仕様として解釈できないタイトル（カレンダー上で直接作られた場合など）
    let unparsable = reservation_event(
        "unparsable",
        "実験中",
        "owner@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let valid = reservation_event(
        "valid",
        "1",
        "owner@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let (repository, _gateway, _mapping_path) = repository_with(vec![unparsable, valid]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let overlapping = repository.find_overlapping(&period).await.unwrap();

    assert_eq!(
        overlapping.len(),
        1,
        "解釈できないイベントは飛ばし、残りは返すべき"
    );
}

#[tokio::test]
async fn find_future_excludes_reservation_that_already_ended() {
    let now = Utc::now();
    let ended = reservation_event(
        "already-ended",
        "0",
        "owner@example.com",
        now - Duration::hours(5),
        now - Duration::hours(3),
    );
    let (repository, _gateway, _mapping_path) = repository_with(vec![ended]);

    let future = repository.find_future().await.unwrap();

    assert!(
        future.is_empty(),
        "終了済みの予約は今後の予約として扱わない"
    );
}

#[tokio::test]
async fn find_future_keeps_ongoing_reservation() {
    let now = Utc::now();
    let ongoing = reservation_event(
        "ongoing",
        "0",
        "owner@example.com",
        now - Duration::hours(5),
        now + Duration::hours(1),
    );
    let (repository, _gateway, _mapping_path) = repository_with(vec![ongoing]);

    let future = repository.find_future().await.unwrap();

    assert_eq!(
        future.len(),
        1,
        "開始時刻が過去でも進行中の予約は含めるべき"
    );
}

#[tokio::test]
async fn parse_failures_are_reported_once_per_event() {
    let (repository, _gateway, _mapping_path) = repository_with(vec![]);

    // 同じイベントの解釈失敗を毎回警告すると、ポーリングのたびにログが積み上がる
    assert!(
        repository.should_report_parse_failure("event-a"),
        "初回は報告する"
    );
    assert!(
        !repository.should_report_parse_failure("event-a"),
        "2回目以降は報告しない"
    );
    assert!(
        repository.should_report_parse_failure("event-b"),
        "別のイベントは報告する"
    );
}
