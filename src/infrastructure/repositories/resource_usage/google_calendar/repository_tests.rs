//! `GoogleCalendarUsageRepository`の検索系メソッドのテスト
//!
//! カレンダーAPIへのアクセスは`CalendarEventGateway`のテストダブルに差し替え、
//! 「どの期間を問い合わせるか」「取得したイベントをどう解釈するか」を検証する。

use super::event_gateway::CalendarEventGateway;
use super::repository::GoogleCalendarUsageRepository;
use crate::domain::aggregates::identity_link::entity::IdentityLink;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::domain::ports::repositories::{
    IdentityLinkRepository, RepositoryError, ResourceUsageRepository,
};
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
        Arc::new(StubIdentityLinkRepository::default()),
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

/// メールアドレスからOSユーザー名を引けるテスト用のIdentityLinkリポジトリ
#[derive(Default)]
struct StubIdentityLinkRepository {
    /// (email, server) -> OSユーザー名
    os_users: Vec<(String, String, String)>,
}

impl StubIdentityLinkRepository {
    fn with_os_user(mut self, email: &str, server: &str, os_user: &str) -> Self {
        self.os_users
            .push((email.to_string(), server.to_string(), os_user.to_string()));
        self
    }
}

#[async_trait]
impl IdentityLinkRepository for StubIdentityLinkRepository {
    async fn find_by_email(
        &self,
        email: &crate::domain::common::EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        let mut link: Option<IdentityLink> = None;
        for (linked_email, server, os_user) in &self.os_users {
            if linked_email != email.as_str() {
                continue;
            }
            let identity = ExternalIdentity::new(
                ExternalSystem::Os {
                    server: server.clone(),
                },
                os_user.clone(),
            );
            link = Some(match link {
                None => IdentityLink::with_external_identity(email.clone(), identity),
                Some(mut existing) => {
                    existing.link_external_identity(identity).unwrap();
                    existing
                }
            });
        }
        Ok(link)
    }

    async fn find_by_external_user_id(
        &self,
        _system: &ExternalSystem,
        _user_id: &str,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }

    async fn save(&self, _identity_link: IdentityLink) -> Result<(), RepositoryError> {
        unimplemented!("このテストでは使用しない")
    }
}

fn repository_with_identities(
    identity_repo: StubIdentityLinkRepository,
) -> (
    GoogleCalendarUsageRepository,
    Arc<FakeCalendarEventGateway>,
    TempMappingPath,
) {
    let gateway = Arc::new(FakeCalendarEventGateway::new(vec![]));
    let mapping_path = TempMappingPath::new();
    let repository = GoogleCalendarUsageRepository::with_gateway(
        gateway.clone(),
        test_config(),
        SERVICE_ACCOUNT_EMAIL.to_string(),
        mapping_path.path(),
        Arc::new(identity_repo),
    )
    .expect("テスト用リポジトリの構築に失敗");

    (repository, gateway, mapping_path)
}

fn gpu_usage(owner: &str, device_number: u32) -> ResourceUsage {
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    let now = Utc::now();
    ResourceUsage::new(
        crate::domain::common::EmailAddress::new(owner.to_string()).unwrap(),
        TimePeriod::new(now, now + Duration::hours(1)).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))],
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn description_includes_the_os_user_name_for_the_reserved_server() {
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "owner@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let usage = gpu_usage("owner@example.com", 0);

    let description = repository.build_description(&usage).await;

    assert!(
        description.contains("OS: kkawaguchi"),
        "予約したサーバーのOSユーザー名が読める形で入るべき: {description}"
    );
}

#[tokio::test]
async fn description_includes_the_reservation_id() {
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default());
    let usage = gpu_usage("owner@example.com", 0);

    let description = repository.build_description(&usage).await;

    assert!(
        description.contains(usage.id().as_str()),
        "予約IDが入るべき: {description}"
    );
}

#[tokio::test]
async fn description_omits_the_os_line_when_no_link_exists() {
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default());
    let usage = gpu_usage("unlinked@example.com", 0);

    let description = repository.build_description(&usage).await;

    assert!(
        !description.contains("OS:"),
        "紐付けがなければOS行を出さない: {description}"
    );
    assert!(
        description.contains("予約者: unlinked@example.com"),
        "予約者行は常に入る: {description}"
    );
}

#[tokio::test]
async fn description_uses_the_os_name_of_the_server_being_reserved() {
    // 同じ利用者がサーバーごとに別のOSユーザー名を持つ場合、予約したサーバーの名前を使う
    let (repository, _gateway, _mapping_path) = repository_with_identities(
        StubIdentityLinkRepository::default()
            .with_os_user("owner@example.com", "Thalys", "kkawaguchi")
            .with_os_user("owner@example.com", "Freccia", "kinji"),
    );
    let usage = gpu_usage("owner@example.com", 0);

    let description = repository.build_description(&usage).await;

    assert!(description.contains("OS: kkawaguchi"), "{description}");
    assert!(!description.contains("kinji"), "{description}");
}

#[tokio::test]
async fn added_metadata_lines_do_not_leak_into_notes() {
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "owner@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let now = Utc::now();
    let usage = ResourceUsage::new(
        crate::domain::common::EmailAddress::new("owner@example.com".to_string()).unwrap(),
        TimePeriod::new(now, now + Duration::hours(1)).unwrap(),
        vec![Resource::Gpu(
            crate::domain::aggregates::resource_usage::value_objects::Gpu::new(
                "Thalys".to_string(),
                0,
                "A100 80GB PCIe".to_string(),
            ),
        )],
        Some("killしても大丈夫です".to_string()),
    )
    .unwrap();

    // 生成したdescriptionを読み戻したとき、管理セクションの行が備考に混ざらないこと
    let description = repository.build_description(&usage).await;
    let notes = GoogleCalendarUsageRepository::extract_notes(&description);

    assert_eq!(notes.as_deref(), Some("killしても大丈夫です"));
}

/// 旧バージョンが書いたdescription（管理セクション内は予約者行のみ）
fn legacy_description_with_markers(owner: &str, notes: &str) -> String {
    format!(
        "[lab-resource-manager:managed-section:begin]\n予約者: {owner}\n[lab-resource-manager:managed-section:end]\n\n{notes}"
    )
}

/// マーカーが導入される前のdescription（1行目が予約者行）
fn legacy_description_without_markers(owner: &str, notes: &str) -> String {
    format!("予約者: {owner}\n{notes}")
}

#[tokio::test]
async fn reads_reservations_written_by_older_versions_with_markers() {
    let now = Utc::now();
    let mut event = reservation_event(
        "legacy-with-markers",
        "0",
        "placeholder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    event.description = Some(legacy_description_with_markers(
        "legacy@example.com",
        "旧形式の備考",
    ));
    let (repository, _gateway, _mapping_path) = repository_with(vec![event]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();
    let found = repository.find_overlapping(&period).await.unwrap();

    assert_eq!(found.len(), 1, "旧形式のイベントも予約として読めるべき");
    assert_eq!(found[0].owner_email().as_str(), "legacy@example.com");
    assert_eq!(found[0].notes().map(String::as_str), Some("旧形式の備考"));
}

#[tokio::test]
async fn reads_reservations_written_before_markers_existed() {
    let now = Utc::now();
    let mut event = reservation_event(
        "legacy-no-markers",
        "1",
        "placeholder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    event.description = Some(legacy_description_without_markers(
        "ancient@example.com",
        "マーカー導入前の備考",
    ));
    let (repository, _gateway, _mapping_path) = repository_with(vec![event]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();
    let found = repository.find_overlapping(&period).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].owner_email().as_str(), "ancient@example.com");
    assert_eq!(
        found[0].notes().map(String::as_str),
        Some("マーカー導入前の備考")
    );
}

#[tokio::test]
async fn newly_written_description_round_trips() {
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "owner@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let now = Utc::now();
    let usage = ResourceUsage::new(
        crate::domain::common::EmailAddress::new("owner@example.com".to_string()).unwrap(),
        TimePeriod::new(now, now + Duration::hours(1)).unwrap(),
        vec![Resource::Gpu(
            crate::domain::aggregates::resource_usage::value_objects::Gpu::new(
                "Thalys".to_string(),
                0,
                "A100 80GB PCIe".to_string(),
            ),
        )],
        Some("実験中".to_string()),
    )
    .unwrap();

    // 新形式で書いたdescriptionから、予約者と備考が元の値として読み戻せること
    let description = repository.build_description(&usage).await;

    let owner = description
        .lines()
        .find_map(|line| line.strip_prefix("予約者: "))
        .expect("予約者行が読めるべき");
    assert_eq!(owner, "owner@example.com");
    assert_eq!(
        GoogleCalendarUsageRepository::extract_notes(&description).as_deref(),
        Some("実験中")
    );
}

#[tokio::test]
async fn metadata_lines_are_not_mistaken_for_the_owner() {
    // OS行や予約ID行が予約者として誤読されないこと（行の走査順に依存しない）
    let (repository, _gateway, _mapping_path) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "owner@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let usage = gpu_usage("owner@example.com", 0);

    let description = repository.build_description(&usage).await;
    let owner_lines: Vec<&str> = description
        .lines()
        .filter_map(|line| line.strip_prefix("予約者: "))
        .collect();

    assert_eq!(
        owner_lines,
        vec!["owner@example.com"],
        "予約者行はちょうど1行であるべき: {description}"
    );
}
