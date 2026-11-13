//! `GoogleCalendarReservationRepository`のテスト
//!
//! カレンダーAPIへのアクセスは`CalendarEventGateway`のテストダブルに差し替え、
//! 「どの期間を問い合わせるか」「取得したイベントをどう解釈するか」を検証する。

use super::event_gateway::CalendarEventGateway;
use super::repository::GoogleCalendarReservationRepository;
use crate::domain::aggregates::identity_link::entity::IdentityLink;
use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::aggregates::reservation::{OccurrenceKey, Reservation, ReservationId};
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, RepositoryError, ReservationRepository, ReservedSlotQuery,
};
use crate::infrastructure::config::{
    DeviceConfig, ResourceConfig, ServerConfig, StorageBackend, StorageConfig,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use google_calendar3::api::{Event, EventCreator, EventDateTime};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(super) const SERVICE_ACCOUNT_EMAIL: &str = "service-account@example.com";
const CALENDAR_ID: &str = "thalys-calendar-id";

/// ゲートウェイが受け取った問い合わせ内容
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListQuery {
    calendar_id: String,
    time_min: DateTime<Utc>,
    time_max: Option<DateTime<Utc>>,
}

/// 本物のカレンダーを、テストに要る範囲だけ真似る偽のゲートウェイ
///
/// 振る舞いは契約テスト（`contract_tests.rs`）で本物と突き合わせる。本物より寛容にはしない。
/// 本物で確かめていない振る舞いを足すときは、契約も足す。
///
/// - カレンダーごとにイベントを持つ。別のカレンダーのイベントは見えない
/// - 一覧は、終了時刻が`time_min`より後、かつ（`time_max`指定時は）開始時刻が`time_max`より前の
///   イベントを返す。繰り返しイベントは展開せず、期間に重なる回があるときだけ返す
/// - 繰り返しイベントの回の開始時刻は、テストが[`Self::with_occurrences`]で与えたもの。
///   与えなければ偽物が自前で数える（[`simple_occurrences`]）。本番の繰り返し計算は使わない
/// - 回のイベントID（`繰り返しイベントのID_回の開始時刻`）は、回がある時刻に限って、規則どおりの
///   回でも取得・書き換え・削除できる。削除した回は、最小限の項目だけを持つ
///   `status: cancelled`の回のイベントとして残る
/// - 書き換え・削除は、イベントのetagが渡された版と一致するときだけ通す（If-Match）。
///   書き込むたびに新しいetagになり、回を変えると繰り返しイベントそのもののetagも変わる
/// - 作るときに、使ったことのあるIDは断る（消したイベントのIDも）
pub(super) struct FakeCalendarEventGateway {
    /// （カレンダーID, イベントID）→ イベントの今の姿（消したものは`None`）
    store: Mutex<HashMap<(String, String), Option<Event>>>,
    /// テストが与えた、繰り返しイベントの回の開始時刻（繰り返しイベントのID → 開始時刻）
    occurrences: HashMap<String, Vec<DateTime<Utc>>>,
    queries: Mutex<Vec<ListQuery>>,
    /// insert_eventで受け取ったイベント（calendar_id, event）
    inserted: Mutex<Vec<(String, Event)>>,
    /// update_eventで受け取った対象（calendar_id, event_id）
    updated: Mutex<Vec<(String, String)>>,
    /// delete_eventで受け取った対象（calendar_id, event_id）
    deleted: Mutex<Vec<(String, String)>>,
    /// 最後に採番したetag
    last_etag: Mutex<u64>,
    /// 最後に採番したイベントID
    last_generated_id: Mutex<u64>,
}

impl FakeCalendarEventGateway {
    /// テスト用カレンダー（`CALENDAR_ID`）に`events`がある状態
    pub(super) fn new(events: Vec<Event>) -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            occurrences: HashMap::new(),
            queries: Mutex::new(Vec::new()),
            inserted: Mutex::new(Vec::new()),
            updated: Mutex::new(Vec::new()),
            deleted: Mutex::new(Vec::new()),
            last_etag: Mutex::new(0),
            last_generated_id: Mutex::new(0),
        }
        .with_events(CALENDAR_ID, events)
    }

    /// `calendar_id`のカレンダーに`events`を足す
    pub(super) fn with_events(self, calendar_id: &str, events: Vec<Event>) -> Self {
        {
            let mut store = self.store.lock().unwrap();
            for event in events {
                let id = event.id.clone().expect("テストイベントにはIDが要る");
                store.insert((calendar_id.to_string(), id), Some(event));
            }
        }
        self
    }

    /// 繰り返しイベント`series_id`の回の開始時刻を与える
    ///
    /// 偽物が自前で数えられない規則（[`simple_occurrences`]を参照）を使うときに与える。
    pub(super) fn with_occurrences(mut self, series_id: &str, starts: Vec<DateTime<Utc>>) -> Self {
        self.occurrences.insert(series_id.to_string(), starts);
        self
    }

    fn new_etag(&self) -> String {
        let mut last = self.last_etag.lock().unwrap();
        *last += 1;
        format!("\"{}\"", last)
    }

    fn new_event_id(&self) -> String {
        let mut last = self.last_generated_id.lock().unwrap();
        *last += 1;
        format!("generated{last}")
    }

    fn key(calendar_id: &str, event_id: &str) -> (String, String) {
        (calendar_id.to_string(), event_id.to_string())
    }

    /// イベントの今の姿
    fn stored(&self, calendar_id: &str, event_id: &str) -> Option<Event> {
        let state = self
            .store
            .lock()
            .unwrap()
            .get(&Self::key(calendar_id, event_id))
            .cloned();
        match state {
            Some(state) => state,
            None => self.regular_instance(calendar_id, event_id),
        }
    }

    fn put(&self, calendar_id: &str, event_id: &str, event: Option<Event>) {
        self.store
            .lock()
            .unwrap()
            .insert(Self::key(calendar_id, event_id), event);
    }

    /// まだ規則どおりの回のイベント（本物は回のイベントIDでも取得できる）。回がある時刻に限る
    fn regular_instance(&self, calendar_id: &str, event_id: &str) -> Option<Event> {
        let (series_id, start) = event_id.rsplit_once('_')?;
        let start = chrono::NaiveDateTime::parse_from_str(start, "%Y%m%dT%H%M%SZ")
            .ok()?
            .and_utc();
        let series = self.stored(calendar_id, series_id)?;
        series.recurrence.as_ref()?;
        if !self.occurrence_starts(&series).contains(&start) {
            return None;
        }
        let (first_start, first_end) = event_period(&series)?;
        let length = first_end - first_start;
        Some(Event {
            id: Some(event_id.to_string()),
            recurrence: None,
            recurring_event_id: Some(series_id.to_string()),
            original_start_time: Some(EventDateTime {
                date_time: Some(start),
                ..Default::default()
            }),
            start: Some(EventDateTime {
                date_time: Some(start),
                ..series.start.clone().unwrap_or_default()
            }),
            end: Some(EventDateTime {
                date_time: Some(start + length),
                ..series.end.clone().unwrap_or_default()
            }),
            etag: Some(format!("\"regular-{event_id}\"")),
            ..series
        })
    }

    /// 繰り返しイベントの回の開始時刻
    fn occurrence_starts(&self, series: &Event) -> Vec<DateTime<Utc>> {
        match self
            .occurrences
            .get(series.id.as_deref().unwrap_or_default())
        {
            Some(starts) => starts.clone(),
            None => simple_occurrences(series),
        }
    }

    /// 繰り返しイベントに、期間に重なる回があるか
    fn occurs_within(
        &self,
        series: &Event,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> bool {
        let Some((first_start, first_end)) = event_period(series) else {
            return false;
        };
        let length = first_end - first_start;
        self.occurrence_starts(series)
            .into_iter()
            .any(|start| start + length > time_min && time_max.is_none_or(|max| start < max))
    }

    /// カレンダーにある、消えていないすべてのイベント
    fn all_events(&self, calendar_id: &str) -> Vec<Event> {
        self.store
            .lock()
            .unwrap()
            .iter()
            .filter(|((calendar, _), _)| calendar == calendar_id)
            .filter_map(|(_, event)| event.clone())
            .collect()
    }

    /// 版が今のetagと一致すれば、今のイベントを返す
    fn check_version(
        &self,
        calendar_id: &str,
        event_id: &str,
        version: &str,
    ) -> Result<Event, RepositoryError> {
        match self.stored(calendar_id, event_id) {
            Some(event) if event.etag.as_deref() == Some(version) => Ok(event),
            _ => Err(RepositoryError::Conflict),
        }
    }

    /// カレンダー上で直接書き換えられたことにする（etagが変わる）
    fn edit_in_calendar(&self, event_id: &str) {
        let mut event = self
            .stored(CALENDAR_ID, event_id)
            .expect("書き換えるイベントが無い");
        event.etag = Some(self.new_etag());
        self.touch_series_of(CALENDAR_ID, &event);
        self.put(CALENDAR_ID, event_id, Some(event));
    }

    /// 回を変えたときの、繰り返しイベントそのものの版の変化
    ///
    /// 本物は、回を書き換えたり削除したりすると、繰り返しイベントそのもののetagも変える。
    fn touch_series_of(&self, calendar_id: &str, instance: &Event) {
        let Some(series_id) = instance.recurring_event_id.as_deref() else {
            return;
        };
        if let Some(mut series) = self.stored(calendar_id, series_id) {
            series.etag = Some(self.new_etag());
            self.put(calendar_id, series_id, Some(series));
        }
    }

    /// カレンダー上で直接消されたことにする
    fn delete_in_calendar(&self, event_id: &str) {
        self.put(CALENDAR_ID, event_id, None);
    }

    fn queries(&self) -> Vec<ListQuery> {
        self.queries.lock().unwrap().clone()
    }

    fn inserted(&self) -> Vec<(String, Event)> {
        self.inserted.lock().unwrap().clone()
    }

    fn updated(&self) -> Vec<(String, String)> {
        self.updated.lock().unwrap().clone()
    }

    fn deleted(&self) -> Vec<(String, String)> {
        self.deleted.lock().unwrap().clone()
    }
}

/// イベントの開始時刻と終了時刻（終日のイベントは、その日のUTCの0時。時刻の無いイベントは`None`）
fn event_period(event: &Event) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let time = |at: &EventDateTime| {
        at.date_time.or_else(|| {
            at.date
                .map(|date| date.and_time(chrono::NaiveTime::MIN).and_utc())
        })
    };
    Some((time(event.start.as_ref()?)?, time(event.end.as_ref()?)?))
}

/// 偽物が自前で数える、繰り返しイベントの回の開始時刻
///
/// 本番の繰り返し計算（`RecurrenceRule`）とは別に、テストで使う単純な規則だけを、現地時刻で
/// 日を足して数える。本番の計算に誤りがあっても、偽物が同じ誤りをしないようにするため。
///
/// 数えられるのは、`FREQ=DAILY`か`FREQ=WEEKLY`に、`INTERVAL`・`COUNT`・`UNTIL`と、最初の回と
/// 同じ曜日だけの`BYDAY`を付けた規則。終わりの無い規則は10年分まで数える。それ以外の規則
/// （EXDATEの行を含む）を使うテストは、[`FakeCalendarEventGateway::with_occurrences`]で回を与える。
fn simple_occurrences(series: &Event) -> Vec<DateTime<Utc>> {
    use chrono::{Datelike, TimeZone};
    let lines = series.recurrence.clone().unwrap_or_default();
    let [line] = lines.as_slice() else {
        panic!("偽物は1行のRRULEしか数えない。with_occurrencesで回を与える: {lines:?}");
    };
    let rule = line
        .strip_prefix("RRULE:")
        .unwrap_or_else(|| panic!("RRULEの行ではない: {line}"));
    let (first_start, _) = event_period(series).expect("繰り返しイベントには時刻が要る");
    let time_zone: chrono_tz::Tz = series
        .start
        .as_ref()
        .and_then(|start| start.time_zone.as_deref())
        .map_or(chrono_tz::UTC, |name| name.parse().expect("時間帯の名前"));
    let first_local = first_start.with_timezone(&time_zone).naive_local();

    let (mut days_per_step, mut interval, mut count, mut until) = (None, 1, None, None);
    for part in rule.split(';') {
        let (name, value) = part.split_once('=').expect("NAME=VALUE");
        match name {
            "FREQ" => {
                days_per_step = Some(match value {
                    "DAILY" => 1,
                    "WEEKLY" => 7,
                    other => panic!("偽物はFREQ={other}を数えない。with_occurrencesで回を与える"),
                })
            }
            "INTERVAL" => interval = value.parse::<i64>().expect("INTERVAL"),
            "COUNT" => count = Some(value.parse::<usize>().expect("COUNT")),
            "UNTIL" => {
                until = Some(
                    chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
                        .expect("UNTIL")
                        .and_utc(),
                )
            }
            "BYDAY" => assert_eq!(
                value,
                ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
                    [first_local.weekday().num_days_from_monday() as usize],
                "偽物は最初の回と同じ曜日のBYDAYしか数えない"
            ),
            other => panic!("偽物は{other}を数えない。with_occurrencesで回を与える"),
        }
    }
    let days_per_step = days_per_step.expect("FREQ") * interval;

    (0..count.unwrap_or(522) as i64)
        .map(|n| first_local + Duration::days(days_per_step * n))
        .map(|local| {
            time_zone
                .from_local_datetime(&local)
                .single()
                .expect("偽物は夏時間の切り替えで重なる・抜ける時刻を数えない")
                .with_timezone(&Utc)
        })
        .take_while(|start| until.is_none_or(|until| *start <= until))
        .collect()
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
            .all_events(calendar_id)
            .into_iter()
            .filter(|event| match &event.recurrence {
                // 本物は、繰り返しイベントを、期間に重なる回があるときだけ返す
                // （回と回の合間の期間には現れない）
                Some(_) => self.occurs_within(event, time_min, time_max),
                None => event_period(event).is_some_and(|(start, end)| {
                    end > time_min && time_max.is_none_or(|max| start < max)
                }),
            })
            .collect())
    }

    async fn list_series(
        &self,
        calendar_id: &str,
        ical_uid: &str,
    ) -> Result<Vec<Event>, RepositoryError> {
        Ok(self
            .all_events(calendar_id)
            .into_iter()
            .filter(|event| event.i_cal_uid.as_deref() == Some(ical_uid))
            .collect())
    }

    async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        Ok(self.stored(calendar_id, event_id))
    }

    async fn insert_event(
        &self,
        calendar_id: &str,
        event: Event,
    ) -> Result<Event, RepositoryError> {
        let mut created = event;
        // 本物はIDを指定しなければ自前で採番し、作成者を書き込んだアカウントで埋める
        let id = match created.id.clone() {
            Some(id) => id,
            None => self.new_event_id(),
        };
        if self
            .store
            .lock()
            .unwrap()
            .contains_key(&Self::key(calendar_id, &id))
        {
            // 本物は、使ったことのあるIDでは作らない（409）
            return Err(RepositoryError::Conflict);
        }
        created.id = Some(id.clone());
        if created.creator.is_none() {
            created.creator = Some(EventCreator {
                email: Some(SERVICE_ACCOUNT_EMAIL.to_string()),
                ..Default::default()
            });
        }
        // 本物は作ったイベントにiCalUIDと作成時刻を付ける
        if created.i_cal_uid.is_none() {
            created.i_cal_uid = Some(format!("{id}@google.com"));
        }
        created.created = Some(Utc::now());
        created.etag = Some(self.new_etag());
        self.put(calendar_id, &id, Some(created.clone()));
        self.inserted
            .lock()
            .unwrap()
            .push((calendar_id.to_string(), created.clone()));
        Ok(created)
    }

    async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: Event,
        version: &str,
    ) -> Result<Event, RepositoryError> {
        let before = self.check_version(calendar_id, event_id, version)?;
        let mut updated = event;
        // 本物は作成者や、繰り返しのどの回かを書き換えない
        updated.id = before.id;
        updated.creator = before.creator;
        updated.created = before.created;
        updated.i_cal_uid = before.i_cal_uid;
        updated.recurring_event_id = before.recurring_event_id;
        updated.original_start_time = before.original_start_time;
        updated.etag = Some(self.new_etag());
        self.touch_series_of(calendar_id, &updated);
        self.put(calendar_id, event_id, Some(updated.clone()));
        self.updated
            .lock()
            .unwrap()
            .push((calendar_id.to_string(), event_id.to_string()));
        Ok(updated)
    }

    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        version: &str,
    ) -> Result<(), RepositoryError> {
        let before = self.check_version(calendar_id, event_id, version)?;
        self.touch_series_of(calendar_id, &before);
        if before.recurring_event_id.is_some() {
            // 本物は、削除した回を、どの回だったかだけを持つ`status: cancelled`の回のイベントとして残す
            let cancelled = Event {
                id: before.id,
                status: Some("cancelled".to_string()),
                etag: Some(self.new_etag()),
                recurring_event_id: before.recurring_event_id,
                original_start_time: before.original_start_time,
                i_cal_uid: before.i_cal_uid,
                ..Default::default()
            };
            self.put(calendar_id, event_id, Some(cancelled));
        } else {
            self.put(calendar_id, event_id, None);
        }
        self.deleted
            .lock()
            .unwrap()
            .push((calendar_id.to_string(), event_id.to_string()));
        Ok(())
    }
}

fn test_config() -> ResourceConfig {
    ResourceConfig {
        servers: vec![ServerConfig {
            name: "Thalys".to_string(),
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

fn test_storage_config() -> StorageConfig {
    let mut calendars = std::collections::HashMap::new();
    calendars.insert("Thalys".to_string(), CALENDAR_ID.to_string());
    StorageConfig {
        backend: StorageBackend::GoogleCalendar { calendars },
    }
}

/// アプリが作成した予約イベント（予約者はdescriptionのアプリ管理セクションに入る）
fn reservation_event(
    event_id: &str,
    gpu_spec: &str,
    holder_email: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Event {
    Event {
        id: Some(event_id.to_string()),
        summary: Some(gpu_spec.to_string()),
        description: Some(format!(
            "[lab-resource-manager:managed-section:begin]\n予約者: {}\n[lab-resource-manager:managed-section:end]",
            holder_email
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
        etag: Some("\"0\"".to_string()),
        ..Default::default()
    }
}

/// 空の偽のカレンダー1つ（サーバー1台分）を使うリポジトリ（リポジトリの契約テスト用）
pub(in crate::infrastructure::repositories::reservation) fn repository_on_an_empty_fake_calendar()
-> GoogleCalendarReservationRepository {
    repository_with(vec![]).0
}

fn repository_with(
    events: Vec<Event>,
) -> (
    GoogleCalendarReservationRepository,
    Arc<FakeCalendarEventGateway>,
) {
    repository_on(FakeCalendarEventGateway::new(events))
}

fn repository_on(
    gateway: FakeCalendarEventGateway,
) -> (
    GoogleCalendarReservationRepository,
    Arc<FakeCalendarEventGateway>,
) {
    let gateway = Arc::new(gateway);
    let repository = GoogleCalendarReservationRepository::with_gateway(
        gateway.clone(),
        test_config(),
        SERVICE_ACCOUNT_EMAIL.to_string(),
        Arc::new(StubIdentityLinkRepository::default()),
        test_storage_config(),
    )
    .expect("テスト用リポジトリの構築に失敗");

    (repository, gateway)
}

#[tokio::test]
async fn find_overlapping_includes_reservation_that_already_ended() {
    let now = Utc::now();
    // 5時間前に始まり3時間前に終わった予約（事後予約の承諾が遅れた場合に相手となるもの）
    let existing = reservation_event(
        "already-ended",
        "0",
        "holder@example.com",
        now - Duration::hours(5),
        now - Duration::hours(3),
    );
    let (repository, _gateway) = repository_with(vec![existing]);

    // 4時間前〜2時間前を予約しようとすると、上の予約と重なる
    let period = TimePeriod::new(now - Duration::hours(4), now - Duration::hours(2)).unwrap();

    let overlapping = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(
        overlapping.len(),
        1,
        "終了時刻が過去の予約も競合チェックの対象に含めるべき"
    );
}

#[tokio::test]
async fn find_overlapping_queries_exactly_the_requested_period() {
    let now = Utc::now();
    let (repository, gateway) = repository_with(vec![]);
    let period = TimePeriod::new(now - Duration::hours(4), now - Duration::hours(2)).unwrap();

    repository.find_slots_overlapping(&period).await.unwrap();

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
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    cancelled.status = Some("cancelled".to_string());
    let (repository, _gateway) = repository_with(vec![cancelled]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let overlapping = repository.find_slots_overlapping(&period).await.unwrap();

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
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let valid = reservation_event(
        "valid",
        "1",
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let (repository, _gateway) = repository_with(vec![unparsable, valid]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let overlapping = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(
        overlapping.len(),
        1,
        "解釈できないイベントは飛ばし、残りは返すべき"
    );
}

#[tokio::test]
async fn find_overlapping_skips_all_day_events_but_keeps_the_rest() {
    let now = Utc::now();
    let mut all_day = reservation_event(
        "all-day",
        "0",
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let today = now.date_naive();
    all_day.start = Some(EventDateTime {
        date: Some(today),
        ..Default::default()
    });
    all_day.end = Some(EventDateTime {
        date: Some(today.succ_opt().unwrap()),
        ..Default::default()
    });
    let timed = reservation_event(
        "timed",
        "1",
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let (repository, _gateway) = repository_with(vec![all_day, timed]);
    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let reservations = repository.find_overlapping(&period).await.unwrap();

    let ids: Vec<&str> = reservations.iter().map(|r| r.id().as_str()).collect();
    assert_eq!(
        ids,
        vec!["timed"],
        "時刻の無い終日の予定は予約として読まない"
    );
}

#[tokio::test]
async fn a_window_starting_now_excludes_reservations_that_already_ended() {
    let now = Utc::now();
    let ended = reservation_event(
        "already-ended",
        "0",
        "holder@example.com",
        now - Duration::hours(5),
        now - Duration::hours(3),
    );
    let (repository, _gateway) = repository_with(vec![ended]);

    let window = TimePeriod::new(now, now + Duration::days(30)).unwrap();
    let future = repository.find_slots_overlapping(&window).await.unwrap();

    assert!(future.is_empty(), "終了済みの予約は今後の期間に含まれない");
}

#[tokio::test]
async fn a_window_starting_now_keeps_an_ongoing_reservation() {
    let now = Utc::now();
    let ongoing = reservation_event(
        "ongoing",
        "0",
        "holder@example.com",
        now - Duration::hours(5),
        now + Duration::hours(1),
    );
    let (repository, _gateway) = repository_with(vec![ongoing]);

    let window = TimePeriod::new(now, now + Duration::days(30)).unwrap();
    let future = repository.find_slots_overlapping(&window).await.unwrap();

    assert_eq!(
        future.len(),
        1,
        "開始時刻が過去でも進行中の予約は含めるべき"
    );
}

#[tokio::test]
async fn parse_failures_are_reported_once_per_event() {
    let (repository, _gateway) = repository_with(vec![]);

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
pub(super) struct StubIdentityLinkRepository {
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
    GoogleCalendarReservationRepository,
    Arc<FakeCalendarEventGateway>,
) {
    let gateway = Arc::new(FakeCalendarEventGateway::new(vec![]));
    let repository = GoogleCalendarReservationRepository::with_gateway(
        gateway.clone(),
        test_config(),
        SERVICE_ACCOUNT_EMAIL.to_string(),
        Arc::new(identity_repo),
        test_storage_config(),
    )
    .expect("テスト用リポジトリの構築に失敗");

    (repository, gateway)
}

fn gpu_usage(holder: &str, device_number: u32) -> Reservation {
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    let now = Utc::now();
    Reservation::one_time(
        crate::domain::common::EmailAddress::new(holder.to_string()).unwrap(),
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
    let (repository, _gateway) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "holder@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let reservation = gpu_usage("holder@example.com", 0);

    let description = repository.build_description(&reservation).await;

    assert!(
        description.contains("OS: kkawaguchi"),
        "予約したサーバーのOSユーザー名が読める形で入るべき: {description}"
    );
}

#[tokio::test]
async fn description_includes_the_reservation_id() {
    let (repository, _gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let reservation = gpu_usage("holder@example.com", 0);

    let description = repository.build_description(&reservation).await;

    assert!(
        description.contains(reservation.id().as_str()),
        "予約IDが入るべき: {description}"
    );
}

#[tokio::test]
async fn description_omits_the_os_line_when_no_link_exists() {
    let (repository, _gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let reservation = gpu_usage("unlinked@example.com", 0);

    let description = repository.build_description(&reservation).await;

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
    let (repository, _gateway) = repository_with_identities(
        StubIdentityLinkRepository::default()
            .with_os_user("holder@example.com", "Thalys", "kkawaguchi")
            .with_os_user("holder@example.com", "Freccia", "kinji"),
    );
    let reservation = gpu_usage("holder@example.com", 0);

    let description = repository.build_description(&reservation).await;

    assert!(description.contains("OS: kkawaguchi"), "{description}");
    assert!(!description.contains("kinji"), "{description}");
}

#[tokio::test]
async fn added_metadata_lines_do_not_leak_into_notes() {
    let (repository, _gateway) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "holder@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let now = Utc::now();
    let reservation = Reservation::one_time(
        crate::domain::common::EmailAddress::new("holder@example.com".to_string()).unwrap(),
        TimePeriod::new(now, now + Duration::hours(1)).unwrap(),
        vec![Resource::Gpu(
            crate::domain::aggregates::reservation::value_objects::Gpu::new(
                "Thalys".to_string(),
                0,
                "A100 80GB PCIe".to_string(),
            ),
        )],
        Some("killしても大丈夫です".to_string()),
    )
    .unwrap();

    // 生成したdescriptionを読み戻したとき、管理セクションの行が備考に混ざらないこと
    let description = repository.build_description(&reservation).await;
    let notes = GoogleCalendarReservationRepository::extract_notes(&description);

    assert_eq!(notes.as_deref(), Some("killしても大丈夫です"));
}

/// 旧バージョンが書いたdescription（管理セクション内は予約者行のみ）
fn legacy_description_with_markers(holder: &str, notes: &str) -> String {
    format!(
        "[lab-resource-manager:managed-section:begin]\n予約者: {holder}\n[lab-resource-manager:managed-section:end]\n\n{notes}"
    )
}

/// マーカーが導入される前のdescription（1行目が予約者行）
fn legacy_description_without_markers(holder: &str, notes: &str) -> String {
    format!("予約者: {holder}\n{notes}")
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
    let (repository, _gateway) = repository_with(vec![event]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();
    let found = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(found.len(), 1, "旧形式のイベントも予約として読めるべき");
    assert_eq!(found[0].holder_email().as_str(), "legacy@example.com");
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
    let (repository, _gateway) = repository_with(vec![event]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();
    let found = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].holder_email().as_str(), "ancient@example.com");
    assert_eq!(
        found[0].notes().map(String::as_str),
        Some("マーカー導入前の備考")
    );
}

#[tokio::test]
async fn newly_written_description_round_trips() {
    let (repository, _gateway) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "holder@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let now = Utc::now();
    let reservation = Reservation::one_time(
        crate::domain::common::EmailAddress::new("holder@example.com".to_string()).unwrap(),
        TimePeriod::new(now, now + Duration::hours(1)).unwrap(),
        vec![Resource::Gpu(
            crate::domain::aggregates::reservation::value_objects::Gpu::new(
                "Thalys".to_string(),
                0,
                "A100 80GB PCIe".to_string(),
            ),
        )],
        Some("実験中".to_string()),
    )
    .unwrap();

    // 新形式で書いたdescriptionから、予約者と備考が元の値として読み戻せること
    let description = repository.build_description(&reservation).await;

    let holder = description
        .lines()
        .find_map(|line| line.strip_prefix("予約者: "))
        .expect("予約者行が読めるべき");
    assert_eq!(holder, "holder@example.com");
    assert_eq!(
        GoogleCalendarReservationRepository::extract_notes(&description).as_deref(),
        Some("実験中")
    );
}

#[tokio::test]
async fn metadata_lines_are_not_mistaken_for_the_holder() {
    // OS行や予約ID行が予約者として誤読されないこと（行の走査順に依存しない）
    let (repository, _gateway) =
        repository_with_identities(StubIdentityLinkRepository::default().with_os_user(
            "holder@example.com",
            "Thalys",
            "kkawaguchi",
        ));
    let reservation = gpu_usage("holder@example.com", 0);

    let description = repository.build_description(&reservation).await;
    let holder_lines: Vec<&str> = description
        .lines()
        .filter_map(|line| line.strip_prefix("予約者: "))
        .collect();

    assert_eq!(
        holder_lines,
        vec!["holder@example.com"],
        "予約者行はちょうど1行であるべき: {description}"
    );
}

#[tokio::test]
async fn adding_a_reservation_uses_an_event_id_derived_from_its_id() {
    let (repository, gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);

    repository.save(&mut reservation).await.unwrap();

    let inserted = gateway.inserted();
    assert_eq!(inserted.len(), 1);
    let event_id = inserted[0].1.id.clone().expect("イベントIDを指定するべき");
    assert_eq!(
        event_id,
        reservation.id().as_str().replace('-', ""),
        "イベントIDは予約IDから導出されるべき"
    );
    assert!(
        event_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "イベントIDに使える文字はbase32hexに限られる: {event_id}"
    );
    assert_eq!(
        reservation.version().map(|v| v.as_str()),
        inserted[0].1.etag.as_deref(),
        "作ったイベントのetagを版として持つべき"
    );
}

#[tokio::test]
async fn a_saved_reservation_can_be_found_by_id_without_a_mapping_file() {
    let (repository, _gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);

    repository.save(&mut reservation).await.unwrap();
    let found = repository.find(reservation.id()).await.unwrap();

    let found = found.expect("保存した予約はIDで引けるべき");
    assert_eq!(found.id(), reservation.id());
    assert_eq!(found.version(), reservation.version());
    assert!(found.has_same_content_as(&reservation));
}

#[tokio::test]
async fn an_event_deleted_in_the_calendar_is_not_found() {
    let now = Utc::now();
    let mut event = reservation_event(
        "deletedevent1",
        "0",
        "holder@example.com",
        now,
        now + Duration::hours(1),
    );
    // 削除されたイベントも、IDを指定すれば`status: cancelled`で返ることがある
    event.status = Some("cancelled".to_string());
    let (repository, _gateway) = repository_with(vec![event]);

    let found = repository
        .find(&ReservationId::from_string("deletedevent1".to_string()))
        .await
        .unwrap();

    assert!(found.is_none(), "{found:?}");
}

#[tokio::test]
async fn reading_an_event_without_a_mapping_uses_the_event_id_as_the_reservation_id() {
    let now = Utc::now();
    let event = reservation_event(
        "someeventid1234",
        "0",
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let (repository, _gateway) = repository_with(vec![event]);

    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();
    let found = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].id().as_str(),
        "someeventid1234",
        "イベントIDをそのまま予約IDとして扱うべき"
    );
}

#[tokio::test]
async fn reading_the_same_event_twice_yields_a_stable_reservation_id() {
    let now = Utc::now();
    let event = reservation_event(
        "stableeventid99",
        "0",
        "holder@example.com",
        now - Duration::hours(1),
        now + Duration::hours(1),
    );
    let (repository, _gateway) = repository_with(vec![event]);
    let period = TimePeriod::new(now - Duration::hours(1), now + Duration::hours(1)).unwrap();

    let first = repository.find_slots_overlapping(&period).await.unwrap();
    let second = repository.find_slots_overlapping(&period).await.unwrap();

    assert_eq!(
        first[0].id(),
        second[0].id(),
        "同じイベントを読むたびにIDが変わってはいけない"
    );
}

#[tokio::test]
async fn saving_a_changed_reservation_updates_its_event_instead_of_inserting_again() {
    let (repository, gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);
    repository.save(&mut reservation).await.unwrap();
    let first_version = reservation.version().cloned();

    // 期間を変えて同じ予約を保存し直す
    let start = reservation
        .slot(&OccurrenceKey::Only)
        .unwrap()
        .time_period()
        .start();
    let holder = reservation.holder_email().clone();
    reservation
        .reschedule_slot(
            &OccurrenceKey::Only,
            TimePeriod::new(start, start + Duration::hours(6)).unwrap(),
            &holder,
        )
        .unwrap();
    repository.save(&mut reservation).await.unwrap();

    assert_eq!(
        gateway.inserted().len(),
        1,
        "既存の予約を作り直してはいけない（同じIDのイベント作成はAPIが拒否する）"
    );
    let updated = gateway.updated();
    assert_eq!(updated.len(), 1, "更新として扱うべき");
    assert_eq!(
        updated[0].1,
        reservation.id().as_str().replace('-', ""),
        "同じイベントIDを更新するべき"
    );
    assert_ne!(
        reservation.version().cloned(),
        first_version,
        "版が進むべき"
    );
}

#[tokio::test]
async fn saving_over_a_change_made_in_the_calendar_is_refused() {
    let (repository, gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);
    repository.save(&mut reservation).await.unwrap();
    // 読んだあとに、誰かがカレンダーでイベントを書き換えた
    gateway.edit_in_calendar(&reservation.id().as_str().replace('-', ""));

    let holder = reservation.holder_email().clone();
    reservation
        .change_slot_notes(&OccurrenceKey::Only, "上書き".to_string(), &holder)
        .unwrap();
    let result = repository.save(&mut reservation).await;

    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "{result:?}"
    );
    assert!(
        gateway.updated().is_empty(),
        "他人の変更を上書きしてはいけない"
    );
}

#[tokio::test]
async fn saving_a_reservation_deleted_in_the_calendar_does_not_create_it_again() {
    let (repository, gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);
    repository.save(&mut reservation).await.unwrap();
    gateway.delete_in_calendar(&reservation.id().as_str().replace('-', ""));

    let holder = reservation.holder_email().clone();
    reservation
        .change_slot_notes(&OccurrenceKey::Only, "編集".to_string(), &holder)
        .unwrap();
    let result = repository.save(&mut reservation).await;

    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "{result:?}"
    );
    assert_eq!(
        gateway.inserted().len(),
        1,
        "消された予約を保存で作り直してはいけない"
    );
}

#[tokio::test]
async fn saving_a_cancelled_reservation_deletes_its_event() {
    let (repository, gateway) = repository_with_identities(StubIdentityLinkRepository::default());
    let mut reservation = gpu_usage("holder@example.com", 0);
    repository.save(&mut reservation).await.unwrap();

    let holder = reservation.holder_email().clone();
    reservation.cancel(&holder).unwrap();
    repository.save(&mut reservation).await.unwrap();

    assert_eq!(
        gateway.deleted(),
        vec![(
            CALENDAR_ID.to_string(),
            reservation.id().as_str().replace('-', "")
        )]
    );
    assert!(repository.find(reservation.id()).await.unwrap().is_none());
}

mod recurring {
    use super::*;
    use crate::domain::aggregates::reservation::{ExceptionChange, Schedule};
    use chrono::TimeZone;

    const SERIES_ID: &str = "weeklyseries1";

    /// 東京の2026年の時刻をUTCで
    fn jst(m: u32, d: u32, h: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, m, d, h, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// 毎週月曜10時〜12時（東京）の繰り返しイベント。最初の回は11月16日
    fn weekly_event() -> Event {
        let mut event = reservation_event(
            SERIES_ID,
            "0",
            "holder@example.com",
            jst(11, 16, 10),
            jst(11, 16, 12),
        );
        event.recurrence = Some(vec!["RRULE:FREQ=WEEKLY;BYDAY=MO".to_string()]);
        event.i_cal_uid = Some(format!("{SERIES_ID}@google.com"));
        event.start.as_mut().unwrap().time_zone = Some("Asia/Tokyo".to_string());
        event
    }

    /// 繰り返しイベントの、本来`original`に始まる回を動かしたイベント
    fn moved_occurrence(original: DateTime<Utc>, start: DateTime<Utc>) -> Event {
        let mut event = reservation_event(
            &format!("{SERIES_ID}_{}", original.format("%Y%m%dT%H%M%SZ")),
            "0",
            "holder@example.com",
            start,
            start + Duration::hours(2),
        );
        event.recurring_event_id = Some(SERIES_ID.to_string());
        event.original_start_time = Some(EventDateTime {
            date_time: Some(original),
            ..Default::default()
        });
        event.i_cal_uid = Some(format!("{SERIES_ID}@google.com"));
        event.etag = Some("\"moved\"".to_string());
        event
    }

    /// 繰り返しイベントの、本来`original`に始まる回を削除したイベント
    ///
    /// 本物と同じく、どの回だったかを示す項目だけを持つ。
    fn cancelled_occurrence(original: DateTime<Utc>) -> Event {
        Event {
            id: Some(format!("{SERIES_ID}_{}", original.format("%Y%m%dT%H%M%SZ"))),
            status: Some("cancelled".to_string()),
            etag: Some("\"cancelled\"".to_string()),
            recurring_event_id: Some(SERIES_ID.to_string()),
            original_start_time: Some(EventDateTime {
                date_time: Some(original),
                ..Default::default()
            }),
            i_cal_uid: Some(format!("{SERIES_ID}@google.com")),
            ..Default::default()
        }
    }

    fn holder() -> crate::domain::common::EmailAddress {
        crate::domain::common::EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn series_id() -> ReservationId {
        ReservationId::from_string(SERIES_ID.to_string())
    }

    fn at(m: u32, d: u32) -> OccurrenceKey {
        OccurrenceKey::At(jst(m, d, 10))
    }

    fn december() -> TimePeriod {
        TimePeriod::new(jst(11, 20, 0), jst(12, 10, 0)).unwrap()
    }

    #[tokio::test]
    async fn a_recurring_event_is_read_as_one_reservation_with_its_exceptions() {
        let (repository, _gateway) = repository_with(vec![
            weekly_event(),
            moved_occurrence(jst(11, 23, 10), jst(11, 24, 15)),
            cancelled_occurrence(jst(11, 30, 10)),
        ]);

        let reservations = repository.find_overlapping(&december()).await.unwrap();

        assert_eq!(reservations.len(), 1, "回ごとではなく1件の予約");
        let reservation = &reservations[0];
        assert!(reservation.is_recurring());
        assert_eq!(reservation.id(), &series_id());
        let Schedule::Recurring(recurring) = reservation.schedule() else {
            panic!("定期予約のはず");
        };
        assert_eq!(recurring.time_zone(), chrono_tz::Asia::Tokyo);
        assert_eq!(
            recurring.exceptions()[&jst(11, 30, 10)].change(),
            &ExceptionChange::Cancelled
        );
        let keys: Vec<OccurrenceKey> = reservation
            .slots_in(&december())
            .iter()
            .map(|slot| slot.key().clone())
            .collect();
        assert_eq!(
            keys,
            vec![at(11, 23), at(12, 7)],
            "取り消した回は無く、動かした回は動かした先にある"
        );
        assert_eq!(
            reservation.slot(&at(11, 23)).unwrap().time_period().start(),
            jst(11, 24, 15)
        );
    }

    #[tokio::test]
    async fn each_slot_of_a_recurring_reservation_has_the_id_of_its_occurrence() {
        let (repository, _gateway) = repository_with(vec![weekly_event()]);

        let slots = repository
            .find_slots_overlapping(&december())
            .await
            .unwrap();

        let ids: Vec<&str> = slots.iter().map(|slot| slot.id().as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "weeklyseries1_20261123T010000Z",
                "weeklyseries1_20261130T010000Z",
                "weeklyseries1_20261207T010000Z",
            ]
        );
    }

    #[tokio::test]
    async fn the_id_of_one_occurrence_is_not_a_reservation_id() {
        let (repository, _gateway) = repository_with(vec![weekly_event()]);

        let found = repository
            .find(&ReservationId::from_string(
                "weeklyseries1_20261123T010000Z".to_string(),
            ))
            .await
            .unwrap();

        assert!(found.is_none(), "{found:?}");
    }

    #[tokio::test]
    async fn an_unreadable_rule_is_not_a_reservation() {
        let mut event = weekly_event();
        event.recurrence = Some(vec!["RRULE:FREQ=SOMETIMES".to_string()]);
        // カレンダーは12月の期間に回があるとして一覧に返す。読めないのはアダプタの側
        let (repository, gateway) = repository_on(
            FakeCalendarEventGateway::new(vec![event])
                .with_occurrences(SERIES_ID, vec![jst(11, 23, 10)]),
        );

        let reservations = repository.find_overlapping(&december()).await.unwrap();

        assert_eq!(gateway.queries().len(), 1, "一覧は引いた");
        assert!(reservations.is_empty(), "{reservations:?}");
    }

    #[tokio::test]
    async fn rescheduling_one_occurrence_writes_only_that_occurrence() {
        let (repository, gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();
        let later = TimePeriod::new(jst(11, 23, 14), jst(11, 23, 16)).unwrap();

        reservation
            .reschedule_slot(&at(11, 23), later.clone(), &holder())
            .unwrap();
        repository.save(&mut reservation).await.unwrap();

        assert_eq!(
            gateway.updated(),
            vec![(
                CALENDAR_ID.to_string(),
                "weeklyseries1_20261123T010000Z".to_string()
            )],
            "繰り返しイベントそのものやほかの回には書かない"
        );
        let stored = repository.find(&series_id()).await.unwrap().unwrap();
        assert_eq!(stored.slot(&at(11, 23)).unwrap().time_period(), &later);
        assert!(stored.has_same_content_as(&reservation));
        let Schedule::Recurring(saved) = reservation.schedule() else {
            panic!("定期予約のはず");
        };
        assert!(saved.unsaved_occurrences().is_empty());
        assert!(
            saved.exceptions()[&jst(11, 23, 10)].version().is_some(),
            "書いた回の版を持つ"
        );
    }

    #[tokio::test]
    async fn cancelling_one_occurrence_deletes_only_that_occurrence() {
        let (repository, gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();

        reservation.cancel_slot(&at(11, 30), &holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();

        assert_eq!(
            gateway.deleted(),
            vec![(
                CALENDAR_ID.to_string(),
                "weeklyseries1_20261130T010000Z".to_string()
            )]
        );
        let stored = repository.find(&series_id()).await.unwrap().unwrap();
        assert!(stored.slot(&at(11, 30)).is_none());
        assert!(stored.slot(&at(12, 7)).is_some(), "ほかの回は残る");
    }

    #[tokio::test]
    async fn an_occurrence_changed_in_the_calendar_meanwhile_is_not_overwritten() {
        let (repository, gateway) = repository_with(vec![
            weekly_event(),
            moved_occurrence(jst(11, 23, 10), jst(11, 24, 15)),
        ]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();
        // 読んだあとに、同じ回を誰かがカレンダーで書き換えた
        gateway.edit_in_calendar("weeklyseries1_20261123T010000Z");

        reservation.cancel_slot(&at(11, 23), &holder()).unwrap();
        let result = repository.save(&mut reservation).await;

        assert!(
            matches!(result, Err(RepositoryError::Conflict)),
            "{result:?}"
        );
        assert!(gateway.deleted().is_empty());
    }

    #[tokio::test]
    async fn a_change_someone_else_made_to_another_occurrence_is_a_conflict() {
        let (repository, gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();
        // 読んだあとに、別の回を誰かが取り消した。カレンダーは繰り返しイベントの版も変える
        let mut someone_else = repository.find(&series_id()).await.unwrap().unwrap();
        someone_else.cancel_slot(&at(12, 7), &holder()).unwrap();
        repository.save(&mut someone_else).await.unwrap();

        reservation.cancel_slot(&at(11, 30), &holder()).unwrap();
        let result = repository.save(&mut reservation).await;

        assert!(
            matches!(result, Err(RepositoryError::Conflict)),
            "読み直してからやり直す: {result:?}"
        );
        let stored = repository.find(&series_id()).await.unwrap().unwrap();
        assert!(
            stored.slot(&at(11, 30)).is_some(),
            "断った取り消しは書かない"
        );
        assert!(stored.slot(&at(12, 7)).is_none(), "別の人の取り消しは残る");
        assert_eq!(gateway.deleted().len(), 1);
    }

    #[tokio::test]
    async fn the_same_reservation_can_be_saved_again_after_saving_an_occurrence() {
        let (repository, _gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();

        reservation.cancel_slot(&at(11, 30), &holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();
        reservation.cancel_slot(&at(12, 7), &holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();

        let stored = repository.find(&series_id()).await.unwrap().unwrap();
        assert!(stored.slot(&at(11, 30)).is_none());
        assert!(stored.slot(&at(12, 7)).is_none());
        assert_eq!(
            stored.version(),
            reservation.version(),
            "保存したあとの版を持つ"
        );
    }

    #[tokio::test]
    async fn changing_two_occurrences_at_once_writes_both() {
        let (repository, gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();
        let later = TimePeriod::new(jst(11, 23, 14), jst(11, 23, 16)).unwrap();

        reservation
            .reschedule_slot(&at(11, 23), later.clone(), &holder())
            .unwrap();
        reservation.cancel_slot(&at(11, 30), &holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();

        assert_eq!(
            gateway.updated(),
            vec![(
                CALENDAR_ID.to_string(),
                "weeklyseries1_20261123T010000Z".to_string()
            )]
        );
        assert_eq!(
            gateway.deleted(),
            vec![(
                CALENDAR_ID.to_string(),
                "weeklyseries1_20261130T010000Z".to_string()
            )]
        );
        let stored = repository.find(&series_id()).await.unwrap().unwrap();
        assert_eq!(stored.slot(&at(11, 23)).unwrap().time_period(), &later);
        assert!(stored.slot(&at(11, 30)).is_none());
        assert_eq!(
            stored.version(),
            reservation.version(),
            "2つの回を書いたあとの、繰り返しイベントの版を持つ"
        );
    }

    #[tokio::test]
    async fn a_series_whose_count_ran_out_before_the_period_is_not_read() {
        let mut event = weekly_event();
        // 11月16日と23日の2回で終わる
        event.recurrence = Some(vec!["RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=2".to_string()]);
        let (repository, _gateway) = repository_with(vec![event]);
        let after_it_ended = TimePeriod::new(jst(11, 28, 0), jst(12, 10, 0)).unwrap();

        let reservations = repository.find_overlapping(&after_it_ended).await.unwrap();
        let slots = repository
            .find_slots_overlapping(&after_it_ended)
            .await
            .unwrap();

        assert!(reservations.is_empty(), "{reservations:?}");
        assert!(slots.is_empty(), "{slots:?}");
    }

    #[tokio::test]
    async fn cancelling_a_recurring_reservation_deletes_the_recurring_event() {
        let (repository, gateway) = repository_with(vec![weekly_event()]);
        let mut reservation = repository.find(&series_id()).await.unwrap().unwrap();

        reservation.cancel(&holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();

        assert_eq!(
            gateway.deleted(),
            vec![(CALENDAR_ID.to_string(), SERIES_ID.to_string())]
        );
        assert!(repository.find(&series_id()).await.unwrap().is_none());
    }
}

mod rooms {
    use super::*;
    use crate::infrastructure::config::RoomConfig;

    const ROOM: &str = "Meeting Room A";
    const ROOM_CALENDAR_ID: &str = "room-calendar-id";

    /// サーバーのカレンダーと部屋のカレンダーを、別々に持つリポジトリ
    fn repository_with_a_room(
        gateway: FakeCalendarEventGateway,
    ) -> (
        GoogleCalendarReservationRepository,
        Arc<FakeCalendarEventGateway>,
    ) {
        let gateway = Arc::new(gateway);
        let mut config = test_config();
        config.rooms = vec![RoomConfig {
            name: ROOM.to_string(),
            notifications: vec![],
        }];
        let mut calendars = HashMap::new();
        calendars.insert("Thalys".to_string(), CALENDAR_ID.to_string());
        calendars.insert(ROOM.to_string(), ROOM_CALENDAR_ID.to_string());
        let repository = GoogleCalendarReservationRepository::with_gateway(
            gateway.clone(),
            config,
            SERVICE_ACCOUNT_EMAIL.to_string(),
            Arc::new(StubIdentityLinkRepository::default()),
            StorageConfig {
                backend: StorageBackend::GoogleCalendar { calendars },
            },
        )
        .expect("テスト用リポジトリの構築に失敗");
        (repository, gateway)
    }

    fn room_reservation(start: DateTime<Utc>) -> Reservation {
        Reservation::one_time(
            crate::domain::common::EmailAddress::new("holder@example.com".to_string()).unwrap(),
            TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
            vec![Resource::Room {
                name: ROOM.to_string(),
            }],
            Some("ゼミ".to_string()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn a_room_reservation_is_written_to_and_read_from_the_room_calendar() {
        let (repository, gateway) = repository_with_a_room(FakeCalendarEventGateway::new(vec![]));
        let start = Utc::now() + Duration::days(1);
        let mut reservation = room_reservation(start);

        repository.save(&mut reservation).await.unwrap();

        let calendars: Vec<String> = gateway
            .inserted()
            .into_iter()
            .map(|(calendar, _)| calendar)
            .collect();
        assert_eq!(calendars, vec![ROOM_CALENDAR_ID.to_string()]);
        let found = repository
            .find(reservation.id())
            .await
            .unwrap()
            .expect("部屋のカレンダーから読める");
        assert!(
            found.has_same_content_as(&reservation),
            "保存: {reservation:?}\n読んだ: {found:?}"
        );
    }

    #[tokio::test]
    async fn reservations_are_read_from_every_calendar() {
        let start = Utc::now() + Duration::days(1);
        let server_event = reservation_event(
            "on-the-server",
            "0",
            "holder@example.com",
            start,
            start + Duration::hours(1),
        );
        let mut room_event = reservation_event(
            "in-the-room",
            ROOM,
            "holder@example.com",
            start,
            start + Duration::hours(1),
        );
        room_event.summary = Some(ROOM.to_string());
        let (repository, _gateway) = repository_with_a_room(
            FakeCalendarEventGateway::new(vec![server_event])
                .with_events(ROOM_CALENDAR_ID, vec![room_event]),
        );
        let period = TimePeriod::new(start, start + Duration::hours(1)).unwrap();

        let mut ids: Vec<String> = repository
            .find_overlapping(&period)
            .await
            .unwrap()
            .iter()
            .map(|r| r.id().as_str().to_string())
            .collect();
        ids.sort();

        assert_eq!(ids, vec!["in-the-room", "on-the-server"]);
    }

    #[tokio::test]
    async fn an_event_in_another_calendar_is_not_the_reservation_of_this_one() {
        // 同じIDのイベントが、どのリソースにも割り当てていないカレンダーにある
        let start = Utc::now() + Duration::days(1);
        let elsewhere = reservation_event(
            "elsewhere",
            "0",
            "holder@example.com",
            start,
            start + Duration::hours(1),
        );
        let (repository, _gateway) = repository_with_a_room(
            FakeCalendarEventGateway::new(vec![])
                .with_events("unrelated-calendar", vec![elsewhere]),
        );

        let found = repository
            .find(&ReservationId::from_string("elsewhere".to_string()))
            .await
            .unwrap();

        assert!(found.is_none(), "{found:?}");
    }
}
