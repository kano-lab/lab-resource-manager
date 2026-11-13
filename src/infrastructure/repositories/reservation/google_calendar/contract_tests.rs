//! Google Calendar APIとの契約テスト
//!
//! アダプタが頼っているGoogle Calendar APIの振る舞いを確かめる。同じテストを、単体テストで
//! 使う偽のゲートウェイ（`FakeCalendarEventGateway`）と、本物のテスト用カレンダーの両方に流す。
//! 偽物が本物とずれていれば、本物での失敗として現れる。そのときは偽物を本物に合わせる。
//!
//! # 本物のカレンダーで流す
//!
//! テスト専用のカレンダーと、そのカレンダーの「予定の変更」権限を持つサービスアカウントを
//! 用意し、`.env.contract.example`を`.env.contract`にコピーして値を入れ、`--ignored`を付けて流す。
//!
//! ```text
//! cp .env.contract.example .env.contract   # 値を入れる
//! cargo test --all-features contract_tests -- --ignored
//! ```
//!
//! 環境変数`LRM_CONTRACT_CALENDAR_ID`・`LRM_CONTRACT_SERVICE_ACCOUNT_KEY`が設定されていれば、
//! そちらを使う（GitHub Actionsではこちら）。
//!
//! # 並列に流しても互いに触れない
//!
//! - イベント・予約のIDは毎回ランダムに作る
//! - 期間で引くテストは、テストごとにランダムに選んだ、1か月〜1年先の日を使う（選んだ日は
//!   標準エラーに出す）。カレンダーは遠い未来の繰り返しを回に展開しない（2100年代では最初の
//!   回しか作らない）ので、何十年も先の日は使えない。偽物では毎回同じ日を使う
//! - 「ちょうどN件」ではなく「自分の作ったものが期待どおりに含まれる」を確かめる
//! - 作ったイベントは、テストが失敗しても最後に消す。それでも残ったもの（実行が途中で
//!   止められた場合など）は、説明欄の印（[`CONTRACT_MARKER`]）で見分け、作られて1日以上
//!   たったものを次の実行の始めに消す
//! - カレンダーは書いた直後の一覧に現れないことがあり、一時的なエラー（429・5xx）も返すため、
//!   読んで確かめるところは待って再試行する

use super::event_gateway::{CalendarEventGateway, GoogleCalendarEventGateway};
use super::repository::GoogleCalendarReservationRepository;
use super::repository_tests::{
    FakeCalendarEventGateway, SERVICE_ACCOUNT_EMAIL, StubIdentityLinkRepository,
};
use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
use crate::domain::aggregates::reservation::{OccurrenceKey, Reservation, ReservationId};
use crate::domain::common::{EmailAddress, TimePeriod};
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::infrastructure::config::{
    DeviceConfig, ResourceConfig, ServerConfig, StorageBackend, StorageConfig,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use google_calendar3::api::{Event, EventDateTime};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// 契約テストが作ったイベントの説明欄に入れる印
const CONTRACT_MARKER: &str = "[lrm-contract-test]";

/// テスト用カレンダーに割り当てるサーバー名
const SERVER: &str = "ContractTestServer";

/// テストの予約者
const HOLDER: &str = "contract-test@example.com";

/// 繰り返しを数える時間帯
const TIME_ZONE: &str = "Asia/Tokyo";

/// 前の実行の残りを、この実行ですでに掃除したか
static SWEPT: AtomicBool = AtomicBool::new(false);

/// 作ったイベントを覚えておくゲートウェイ（後片付けのため）
struct Recording {
    inner: Arc<dyn CalendarEventGateway>,
    /// 作ったイベント（カレンダーID, イベントID）
    created: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl CalendarEventGateway for Recording {
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError> {
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
        let created = self.inner.insert_event(calendar_id, event).await?;
        if let Some(id) = created.id.clone() {
            self.created
                .lock()
                .unwrap()
                .push((calendar_id.to_string(), id));
        }
        Ok(created)
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

/// 契約テストの相手（偽のゲートウェイか、本物のテスト用カレンダー）
struct Calendar {
    gateway: Arc<Recording>,
    calendar_id: String,
    service_account_email: String,
    /// 本物のカレンダーか。本物は書いた直後の一覧に現れないことがあるので待つ
    real: bool,
}

impl Calendar {
    fn on(
        gateway: Arc<dyn CalendarEventGateway>,
        calendar_id: String,
        service_account_email: String,
        real: bool,
    ) -> Self {
        Self {
            gateway: Arc::new(Recording {
                inner: gateway,
                created: Mutex::new(Vec::new()),
            }),
            calendar_id,
            service_account_email,
            real,
        }
    }

    fn fake() -> Self {
        Self::on(
            Arc::new(FakeCalendarEventGateway::new(Vec::new())),
            "contract-test-calendar".to_string(),
            SERVICE_ACCOUNT_EMAIL.to_string(),
            false,
        )
    }

    async fn real() -> Self {
        // アプリの起動時と同じ暗号化プロバイダを使う。rustlsはringとaws-lc-rsの両方を
        // 持つため、選ばないとTLSの準備で止まる。並列のテストが先に入れていてもよい
        let _ = rustls::crypto::ring::default_provider().install_default();
        let calendar_id = setting("LRM_CONTRACT_CALENDAR_ID").expect(
            "LRM_CONTRACT_CALENDAR_IDにテスト用カレンダーのIDを設定してください（.env.contract）",
        );
        let key = setting("LRM_CONTRACT_SERVICE_ACCOUNT_KEY").expect(
            "LRM_CONTRACT_SERVICE_ACCOUNT_KEYにサービスアカウントの鍵ファイルのパスを設定してください（.env.contract）",
        );
        // 相対パスはリポジトリ直下から数える
        let key = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(key)
            .to_string_lossy()
            .into_owned();
        let (gateway, service_account_email) = GoogleCalendarEventGateway::connect(&key)
            .await
            .expect("サービスアカウントでGoogle Calendar APIにつなげません");
        let calendar = Self::on(Arc::new(gateway), calendar_id, service_account_email, true);
        // 並列に走るテストのうち、最初の1つだけが掃除する
        if !SWEPT.swap(true, Ordering::SeqCst) {
            calendar.sweep_leftovers().await;
        }
        calendar
    }

    /// このカレンダーをサーバー1台分のカレンダーとして使うリポジトリ
    fn repository(&self) -> GoogleCalendarReservationRepository {
        let mut calendars = HashMap::new();
        calendars.insert(SERVER.to_string(), self.calendar_id.clone());
        GoogleCalendarReservationRepository::with_gateway(
            self.gateway.clone(),
            ResourceConfig {
                servers: vec![ServerConfig {
                    name: SERVER.to_string(),
                    devices: vec![DeviceConfig {
                        id: 0,
                        model: "A100".to_string(),
                    }],
                    notifications: vec![],
                }],
                rooms: vec![],
            },
            self.service_account_email.clone(),
            Arc::new(StubIdentityLinkRepository::default()),
            StorageConfig {
                backend: StorageBackend::GoogleCalendar { calendars },
            },
        )
        .expect("リポジトリを組み立てられません")
    }

    /// テストで使う日の、東京の10時
    ///
    /// 本物では、1か月〜1年先からテストごとにランダムに選ぶ。偽物では毎回同じ日（月曜）。
    fn some_day(&self) -> DateTime<Utc> {
        let date = if self.real {
            let days = 30 + (uuid::Uuid::new_v4().as_u128() % 335) as i64;
            (Utc::now() + Duration::days(days))
                .with_timezone(&chrono_tz::Asia::Tokyo)
                .date_naive()
        } else {
            NaiveDate::from_ymd_opt(2027, 6, 7).unwrap()
        };
        let day = local_time(chrono_tz::Asia::Tokyo, date, 10);
        if self.real {
            eprintln!("契約テストの日: {day}");
        }
        day
    }

    /// 夏時間の切り替えをまたぐ1週間の始まりの日（ニューヨーク）
    ///
    /// 本物では、1か月より先で最初の切り替えの3日前。偽物では毎回同じ日（2027年11月7日の
    /// 夏時間の終わりの3日前）。
    fn a_date_before_a_dst_change(&self) -> NaiveDate {
        let new_york = chrono_tz::America::New_York;
        let date = if self.real {
            let mut date = (Utc::now() + Duration::days(30)).date_naive();
            while local_time(new_york, date, 12)
                - local_time(new_york, date.pred_opt().unwrap(), 12)
                == Duration::days(1)
            {
                date = date.succ_opt().unwrap();
            }
            date - Duration::days(3)
        } else {
            NaiveDate::from_ymd_opt(2027, 11, 4).unwrap()
        };
        if self.real {
            eprintln!("契約テストの夏時間をまたぐ週: {date}から");
        }
        date
    }

    /// `attempt`が値を返すまで待って再試行する（偽物は待たない）
    ///
    /// 本物は書いた直後の一覧に現れないことがあり、一時的なエラーも返すので、どちらも待つ。
    async fn eventually<T, F, Fut>(&self, what: &str, mut attempt: F) -> T
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<Option<T>, RepositoryError>>,
    {
        let tries = if self.real { 8 } else { 1 };
        let mut last_error = None;
        for i in 0..tries {
            match attempt().await {
                Ok(Some(found)) => return found,
                Ok(None) => {}
                Err(error) => last_error = Some(error),
            }
            if self.real {
                tokio::time::sleep(std::time::Duration::from_secs(1 << i.min(4))).await;
            }
        }
        panic!("{what}: 待っても確かめられませんでした（最後のエラー: {last_error:?}）");
    }

    /// このテストが作ったイベントを、今の版で消す（繰り返しイベントを消せば回も消える）
    async fn remove_everything_created(&self) {
        let created = self.gateway.created.lock().unwrap().clone();
        for (calendar_id, event_id) in created {
            if let Ok(Some(event)) = self.gateway.get_event(&calendar_id, &event_id).await
                && event.status.as_deref() != Some("cancelled")
                && let Some(etag) = event.etag
            {
                let _ = self
                    .gateway
                    .delete_event(&calendar_id, &event_id, &etag)
                    .await;
            }
        }
    }

    /// 前の実行が途中で止まって残したテスト用イベントを消す
    async fn sweep_leftovers(&self) {
        let Ok(events) = self
            .gateway
            .list_events(&self.calendar_id, Utc::now() - Duration::days(1), None)
            .await
        else {
            return;
        };
        let a_day_ago = Utc::now() - Duration::days(1);
        for event in events {
            let ours = event
                .description
                .as_deref()
                .is_some_and(|description| description.contains(CONTRACT_MARKER));
            let stale = event.created.is_some_and(|created| created < a_day_ago);
            let alive = event.status.as_deref() != Some("cancelled");
            if ours
                && stale
                && alive
                && event.recurring_event_id.is_none()
                && let (Some(id), Some(etag)) = (event.id.as_deref(), event.etag.as_deref())
            {
                let _ = self.gateway.delete_event(&self.calendar_id, id, etag).await;
            }
        }
    }
}

/// 契約のシナリオを流し、成否にかかわらず作ったイベントを消す
async fn run<F, Fut>(calendar: Calendar, scenario: F)
where
    F: FnOnce(Arc<Calendar>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let calendar = Arc::new(calendar);
    let outcome = tokio::spawn(scenario(calendar.clone())).await;
    calendar.remove_everything_created().await;
    if let Err(error) = outcome {
        match error.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(error) => panic!("{error}"),
        }
    }
}

/// `time_zone`での`date`の`hour`時（UTC）
fn local_time(time_zone: Tz, date: NaiveDate, hour: u32) -> DateTime<Utc> {
    time_zone
        .from_local_datetime(&date.and_hms_opt(hour, 0, 0).unwrap())
        .single()
        .expect("その時刻はひとつに決まる")
        .with_timezone(&Utc)
}

/// 設定を、環境変数から、無ければリポジトリ直下の`.env.contract`から読む
fn setting(name: &str) -> Option<String> {
    // 貼り付けた値の前後に改行や空白が付いていても使えるようにする
    if let Ok(value) = std::env::var(name)
        && !value.trim().is_empty()
    {
        return Some(value.trim().to_string());
    }
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env.contract");
    std::fs::read_to_string(file)
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

fn holder() -> EmailAddress {
    EmailAddress::new(HOLDER.to_string()).unwrap()
}

fn gpu() -> Resource {
    Resource::Gpu(Gpu::new(SERVER.to_string(), 0, "A100".to_string()))
}

/// 新しいイベントID（base32hexに収まる、ハイフン無しのUUID）
fn new_event_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// 繰り返しイベントの、本来`start`に始まる回のイベントID
fn instance_id(series_id: &str, start: DateTime<Utc>) -> String {
    format!("{series_id}_{}", start.format("%Y%m%dT%H%M%SZ"))
}

fn at(time: DateTime<Utc>) -> EventDateTime {
    EventDateTime {
        date_time: Some(time),
        time_zone: Some(TIME_ZONE.to_string()),
        ..Default::default()
    }
}

/// アプリが書くのと同じ形の予約イベント（予約者は説明欄のアプリ管理セクションに入る）
fn reservation_event(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Event {
    Event {
        id: Some(id.to_string()),
        summary: Some("0".to_string()),
        description: Some(format!(
            "[lab-resource-manager:managed-section:begin]\n予約者: {HOLDER}\n\
             [lab-resource-manager:managed-section:end]\n\n{CONTRACT_MARKER}"
        )),
        start: Some(at(start)),
        end: Some(at(end)),
        ..Default::default()
    }
}

/// 毎週、`first_start`から2時間を4回繰り返す繰り返しイベント
fn weekly_event(id: &str, first_start: DateTime<Utc>) -> Event {
    Event {
        recurrence: Some(vec!["RRULE:FREQ=WEEKLY;COUNT=4".to_string()]),
        ..reservation_event(id, first_start, first_start + Duration::hours(2))
    }
}

fn original_start(event: &Event) -> Option<DateTime<Utc>> {
    event
        .original_start_time
        .as_ref()
        .and_then(|original| original.date_time)
}

fn is_cancelled(event: &Event) -> bool {
    event.status.as_deref() == Some("cancelled")
}

/// 版（etag）を条件にした書き換え・削除は、版が違えば断られる
async fn conditional_writes(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let start = calendar.some_day();

    let created = gateway
        .insert_event(
            calendar_id,
            reservation_event(&id, start, start + Duration::hours(1)),
        )
        .await
        .unwrap();
    let first = created.etag.expect("作ったイベントにはetagがある");

    let longer = reservation_event(&id, start, start + Duration::hours(2));
    let updated = gateway
        .update_event(calendar_id, &id, longer.clone(), &first)
        .await
        .unwrap();
    let second = updated.etag.expect("書き換えたイベントにはetagがある");
    assert_ne!(first, second, "書き換えるとetagが変わる");

    let stale_update = gateway
        .update_event(calendar_id, &id, longer.clone(), &first)
        .await;
    assert!(
        matches!(stale_update, Err(RepositoryError::Conflict)),
        "古い版での書き換えは断られる: {stale_update:?}"
    );
    let stale_delete = gateway.delete_event(calendar_id, &id, &first).await;
    assert!(
        matches!(stale_delete, Err(RepositoryError::Conflict)),
        "古い版での削除は断られる: {stale_delete:?}"
    );

    gateway
        .delete_event(calendar_id, &id, &second)
        .await
        .unwrap();
    let gone = gateway.get_event(calendar_id, &id).await.unwrap();
    assert!(
        gone.as_ref().is_none_or(is_cancelled),
        "消したイベントは無いか、削除済みとして返る: {gone:?}"
    );
    let resurrect = gateway
        .update_event(calendar_id, &id, longer, &second)
        .await;
    assert!(
        matches!(resurrect, Err(RepositoryError::Conflict)),
        "消したイベントを、消す前の版で書き換えて生き返らせてはいけない: {resurrect:?}"
    );
}

/// 繰り返しイベントは、期間に重なる回があるときだけ一覧に現れる
///
/// 回と回の合間の期間には現れない。予約の変化を監視する範囲から外れた定期予約は、
/// 変化の読み取りが「まだ続いている予約」として持ち越す。
async fn a_recurring_event_is_listed_only_around_its_occurrences(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let first = calendar.some_day();
    gateway
        .insert_event(calendar_id, weekly_event(&id, first))
        .await
        .unwrap();

    // 2回目（7日後）に重なる期間には現れる
    let second = first + Duration::days(7);
    let (from, to) = (second - Duration::hours(1), second + Duration::hours(1));
    let series_id = id.as_str();
    calendar
        .eventually(
            "2回目に重なる期間の一覧に、繰り返しイベントが現れる",
            || async move {
                let events = gateway.list_events(calendar_id, from, Some(to)).await?;
                Ok(events
                    .iter()
                    .any(|event| event.id.as_deref() == Some(series_id) && !is_cancelled(event))
                    .then_some(()))
            },
        )
        .await;

    // 1回目と2回目の間の1日には現れない
    let between = gateway
        .list_events(
            calendar_id,
            first + Duration::days(2),
            Some(first + Duration::days(3)),
        )
        .await
        .unwrap();
    assert!(
        !between
            .iter()
            .any(|event| event.id.as_deref() == Some(series_id)),
        "回と回の合間の期間には現れない"
    );
}

/// 1つの回を、回のイベントIDで、版を条件に書き換え・削除できる
async fn one_occurrence_is_written_by_its_instance_id(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let first = calendar.some_day();
    let series = gateway
        .insert_event(calendar_id, weekly_event(&id, first))
        .await
        .unwrap();
    let series_version = series.etag.expect("繰り返しイベントにはetagがある");
    let ical_uid = series.i_cal_uid.expect("繰り返しイベントにはiCalUIDがある");
    let second = first + Duration::days(7);
    let third = first + Duration::days(14);

    // 規則どおりの回も、回のイベントIDで読める
    let regular = gateway
        .get_event(calendar_id, &instance_id(&id, second))
        .await
        .unwrap()
        .expect("規則どおりの回も、回のイベントIDで読める");
    assert_eq!(regular.recurring_event_id.as_deref(), Some(id.as_str()));
    let regular_version = regular.etag.expect("回にもetagがある");

    // 2回目の時間を変える
    let mut moved = reservation_event(
        &instance_id(&id, second),
        second + Duration::hours(3),
        second + Duration::hours(5),
    );
    moved.recurring_event_id = Some(id.clone());
    moved.original_start_time = Some(at(second));
    let saved = gateway
        .update_event(
            calendar_id,
            &instance_id(&id, second),
            moved.clone(),
            &regular_version,
        )
        .await
        .unwrap();
    assert_ne!(saved.etag.as_deref(), Some(regular_version.as_str()));
    let stale = gateway
        .update_event(
            calendar_id,
            &instance_id(&id, second),
            moved,
            &regular_version,
        )
        .await;
    assert!(
        matches!(stale, Err(RepositoryError::Conflict)),
        "回も、古い版での書き換えは断られる: {stale:?}"
    );

    // 3回目を取り消す
    let third_version = gateway
        .get_event(calendar_id, &instance_id(&id, third))
        .await
        .unwrap()
        .and_then(|instance| instance.etag)
        .expect("3回目のetag");
    gateway
        .delete_event(calendar_id, &instance_id(&id, third), &third_version)
        .await
        .unwrap();

    // 回を変えると、繰り返しイベントそのものの版も変わる。アダプタはこれを頼りに、
    // 読んだあとに同じ定期予約のどの回が変わっても衝突として断る
    let series_now = gateway
        .get_event(calendar_id, &id)
        .await
        .unwrap()
        .expect("繰り返しイベントは残っている");
    assert_ne!(
        series_now.etag.as_deref(),
        Some(series_version.as_str()),
        "1つの回を変えると、繰り返しイベントそのものの版も変わる"
    );

    // 変えた回と取り消した回が、繰り返しイベントの回のイベントとして読める
    let ical_uid = ical_uid.as_str();
    let events = calendar
        .eventually(
            "変えた回と取り消した回が、回のイベントとして読める",
            || async move {
                let events = gateway.list_series(calendar_id, ical_uid).await?;
                let moved = events
                    .iter()
                    .any(|event| original_start(event) == Some(second) && !is_cancelled(event));
                let cancelled = events
                    .iter()
                    .any(|event| original_start(event) == Some(third) && is_cancelled(event));
                Ok((moved && cancelled).then_some(events))
            },
        )
        .await;
    let moved = events
        .iter()
        .find(|event| original_start(event) == Some(second))
        .unwrap();
    assert_eq!(
        moved.start.as_ref().and_then(|start| start.date_time),
        Some(second + Duration::hours(3))
    );
}

/// 単発予約をアダプタ越しに作り、読み、書き換え、取り消す
async fn a_one_time_reservation_round_trips(calendar: &Calendar) {
    let repository = calendar.repository();
    let start = calendar.some_day();
    let mut reservation = Reservation::one_time(
        holder(),
        TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
        vec![gpu()],
        Some(CONTRACT_MARKER.to_string()),
    )
    .unwrap();
    repository.save(&mut reservation).await.unwrap();

    let mut found = repository
        .find(reservation.id())
        .await
        .unwrap()
        .expect("保存した予約を読める");
    assert!(
        found.has_same_content_as(&reservation),
        "保存したとおりに読める\n保存: {reservation:?}\n読んだ: {found:?}"
    );
    let mut stale = found.clone();

    let longer = TimePeriod::new(start, start + Duration::hours(2)).unwrap();
    found
        .reschedule_slot(&OccurrenceKey::Only, longer.clone(), &holder())
        .unwrap();
    repository.save(&mut found).await.unwrap();
    let reread = repository
        .find(reservation.id())
        .await
        .unwrap()
        .expect("書き換えた予約を読める");
    assert_eq!(
        reread.slot(&OccurrenceKey::Only).unwrap().time_period(),
        &longer,
        "書き換えた時間で読める: {reread:?}"
    );
    assert_eq!(
        reread.version(),
        found.version(),
        "保存したあとの版を覚えている"
    );

    stale
        .change_slot_notes(
            &OccurrenceKey::Only,
            format!("{CONTRACT_MARKER} 古い版"),
            &holder(),
        )
        .unwrap();
    let result = repository.save(&mut stale).await;
    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "読んだあとに変わった予約は書き戻さない: {result:?}"
    );

    found.cancel(&holder()).unwrap();
    repository.save(&mut found).await.unwrap();
    assert!(repository.find(reservation.id()).await.unwrap().is_none());
}

/// 定期予約をアダプタ越しに読み、1つの回を変え・取り消し、予約ごと取り消す
async fn a_recurring_reservation_round_trips(calendar: &Calendar) {
    let repository = calendar.repository();
    let id = new_event_id();
    let first = calendar.some_day();
    calendar
        .gateway
        .insert_event(&calendar.calendar_id, weekly_event(&id, first))
        .await
        .unwrap();
    let reservation_id = ReservationId::from_string(id.clone());
    let second = OccurrenceKey::At(first + Duration::days(7));
    let third = OccurrenceKey::At(first + Duration::days(14));

    let mut reservation = repository
        .find(&reservation_id)
        .await
        .unwrap()
        .expect("繰り返しイベントを定期予約として読める");
    assert!(reservation.is_recurring());
    reservation.cancel_slot(&second, &holder()).unwrap();
    reservation
        .change_slot_notes(&third, format!("{CONTRACT_MARKER} この回だけ"), &holder())
        .unwrap();
    repository.save(&mut reservation).await.unwrap();

    let repository_ref = &repository;
    let (reservation_ref, second_ref, third_ref) = (&reservation_id, &second, &third);
    let stored = calendar
        .eventually("変えた回と取り消した回が読める", || async move {
            let Some(stored) = repository_ref.find(reservation_ref).await? else {
                return Ok(None);
            };
            let changed = stored
                .slot(third_ref)
                .and_then(|slot| slot.notes().cloned())
                .is_some_and(|notes| notes.contains("この回だけ"));
            Ok((stored.slot(second_ref).is_none() && changed).then_some(stored))
        })
        .await;
    assert!(
        stored.slot(&OccurrenceKey::At(first)).is_some(),
        "ほかの回は残る"
    );

    // 4回目に重なる期間で、定期予約として読める
    let fourth = first + Duration::days(21);
    let around = TimePeriod::new(fourth - Duration::hours(1), fourth + Duration::hours(1)).unwrap();
    let around_ref = &around;
    calendar
        .eventually(
            "4回目に重なる期間で定期予約が読める",
            || async move {
                Ok(repository_ref
                    .find_overlapping(around_ref)
                    .await?
                    .iter()
                    .any(|found| found.id() == reservation_ref)
                    .then_some(()))
            },
        )
        .await;

    let mut latest = stored;
    latest.cancel(&holder()).unwrap();
    repository.save(&mut latest).await.unwrap();
    assert!(repository.find(&reservation_id).await.unwrap().is_none());
}

/// 無いイベントは見つからない
///
/// 作ったことのないID、繰り返しイベントの規則に無い時刻の回のIDは、どちらも見つからない
/// （ゲートウェイは404を`None`にする）。
async fn an_event_that_does_not_exist_is_not_found(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();

    let never_created = gateway
        .get_event(calendar_id, &new_event_id())
        .await
        .unwrap();
    assert!(never_created.is_none(), "{never_created:?}");

    let id = new_event_id();
    let first = calendar.some_day();
    gateway
        .insert_event(calendar_id, weekly_event(&id, first))
        .await
        .unwrap();
    let off_the_rule = gateway
        .get_event(calendar_id, &instance_id(&id, first + Duration::days(8)))
        .await
        .unwrap();
    assert!(
        off_the_rule.is_none(),
        "規則に無い時刻の回は無い: {off_the_rule:?}"
    );
    let beyond_the_count = gateway
        .get_event(calendar_id, &instance_id(&id, first + Duration::days(28)))
        .await
        .unwrap();
    assert!(
        beyond_the_count.is_none(),
        "回数を超えた回は無い: {beyond_the_count:?}"
    );
}

/// 使ったことのあるIDでは作れない（消したイベントのIDも）
async fn an_id_that_was_used_cannot_be_used_again(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let start = calendar.some_day();
    let event = || reservation_event(&id, start, start + Duration::hours(1));
    let created = gateway.insert_event(calendar_id, event()).await.unwrap();

    let while_it_exists = gateway.insert_event(calendar_id, event()).await;
    assert!(
        matches!(while_it_exists, Err(RepositoryError::Conflict)),
        "{while_it_exists:?}"
    );

    gateway
        .delete_event(calendar_id, &id, &created.etag.unwrap())
        .await
        .unwrap();
    let after_deleting = gateway.insert_event(calendar_id, event()).await;
    assert!(
        matches!(after_deleting, Err(RepositoryError::Conflict)),
        "消したイベントのIDでも作れない: {after_deleting:?}"
    );
}

/// 一覧には、期間に接するだけのイベントは現れない
///
/// `time_min`ちょうどに終わるイベントも、`time_max`ちょうどに始まるイベントも含まない。
async fn listing_leaves_out_events_that_only_touch_the_period(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let start = calendar.some_day();
    let end = start + Duration::hours(1);
    gateway
        .insert_event(calendar_id, reservation_event(&id, start, end))
        .await
        .unwrap();
    let event_id = id.as_str();
    let listed = |from: DateTime<Utc>, to: DateTime<Utc>| async move {
        Ok::<_, RepositoryError>(
            gateway
                .list_events(calendar_id, from, Some(to))
                .await?
                .iter()
                .any(|event| event.id.as_deref() == Some(event_id)),
        )
    };

    // 書いた直後は一覧に現れないことがあるので、重なる期間で現れるのを待ってから確かめる
    calendar
        .eventually("重なる期間の一覧に現れる", || async move {
            Ok(listed(end - Duration::minutes(1), end + Duration::hours(1))
                .await?
                .then_some(()))
        })
        .await;
    assert!(
        !listed(end, end + Duration::hours(1)).await.unwrap(),
        "終わる時刻から始まる期間には現れない"
    );
    assert!(
        !listed(start - Duration::hours(1), start).await.unwrap(),
        "始まる時刻で終わる期間には現れない"
    );
}

/// 動かした回だけが期間にあるとき、その回は一覧に現れ、アダプタはその予約を読める
async fn a_moved_occurrence_alone_in_the_period_is_found(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let id = new_event_id();
    let first = calendar.some_day();
    gateway
        .insert_event(calendar_id, weekly_event(&id, first))
        .await
        .unwrap();
    // 2回目を、3回目と4回目の間（どの回とも重ならない日）へ動かす
    let second = first + Duration::days(7);
    let moved_to = first + Duration::days(17);
    let regular = gateway
        .get_event(calendar_id, &instance_id(&id, second))
        .await
        .unwrap()
        .expect("2回目");
    let mut moved = reservation_event(
        &instance_id(&id, second),
        moved_to,
        moved_to + Duration::hours(2),
    );
    moved.recurring_event_id = Some(id.clone());
    moved.original_start_time = Some(at(second));
    gateway
        .update_event(
            calendar_id,
            &instance_id(&id, second),
            moved,
            &regular.etag.unwrap(),
        )
        .await
        .unwrap();

    let period =
        TimePeriod::new(moved_to - Duration::hours(1), moved_to + Duration::hours(1)).unwrap();
    let instance = instance_id(&id, second);
    let (period_ref, instance_ref) = (&period, instance.as_str());
    calendar
        .eventually("動かした回が一覧に現れる", || async move {
            Ok(gateway
                .list_events(calendar_id, period_ref.start(), Some(period_ref.end()))
                .await?
                .iter()
                .any(|event| event.id.as_deref() == Some(instance_ref) && !is_cancelled(event))
                .then_some(()))
        })
        .await;

    let repository = calendar.repository();
    let reservations = repository.find_overlapping(&period).await.unwrap();
    let reservation = reservations
        .iter()
        .find(|found| found.id().as_str() == id)
        .expect("動かした回だけが期間にある定期予約も読める");
    let slots = reservation.slots_in(&period);
    assert_eq!(slots.len(), 1, "{slots:?}");
    assert_eq!(slots[0].key(), &OccurrenceKey::At(second));
    assert_eq!(slots[0].time_period().start(), moved_to);
}

/// 夏時間のある時間帯の毎週の繰り返しは、現地の同じ時刻に回を作る
///
/// 切り替えをまたぐと、回の開始時刻（UTC）は1時間ずれ、回のイベントIDもその時刻になる。
async fn a_weekly_event_keeps_its_local_time_across_daylight_saving_time(calendar: &Calendar) {
    let gateway = &calendar.gateway;
    let calendar_id = calendar.calendar_id.as_str();
    let new_york = chrono_tz::America::New_York;
    let date = calendar.a_date_before_a_dst_change();
    let first = local_time(new_york, date, 10);
    let second = local_time(new_york, date + Duration::days(7), 10);
    assert_ne!(second - first, Duration::days(7), "切り替えをまたいでいる");
    let id = new_event_id();
    let in_new_york = |time: DateTime<Utc>| EventDateTime {
        date_time: Some(time),
        time_zone: Some("America/New_York".to_string()),
        ..Default::default()
    };
    let series = Event {
        start: Some(in_new_york(first)),
        end: Some(in_new_york(first + Duration::hours(1))),
        ..weekly_event(&id, first)
    };
    gateway.insert_event(calendar_id, series).await.unwrap();

    let instance = gateway
        .get_event(calendar_id, &instance_id(&id, second))
        .await
        .unwrap()
        .expect("2回目は、切り替えのあとの現地10時（UTCでは1時間ずれた時刻）の回");
    assert_eq!(
        instance.start.as_ref().and_then(|start| start.date_time),
        Some(second)
    );

    let reservation = calendar
        .repository()
        .find(&ReservationId::from_string(id.clone()))
        .await
        .unwrap()
        .expect("定期予約として読める");
    let slot = reservation
        .slot(&OccurrenceKey::At(second))
        .expect("アダプタも同じ時刻に回を数える");
    assert_eq!(slot.time_period().start(), second);
    assert!(
        reservation
            .slot(&OccurrenceKey::At(first + Duration::days(7)))
            .is_none()
    );
}

/// 契約を、偽のゲートウェイと本物のカレンダーの両方で確かめるテストにする
macro_rules! contract {
    ($name:ident) => {
        mod $name {
            use super::*;

            #[tokio::test]
            async fn with_the_fake_gateway() {
                run(Calendar::fake(), |calendar| async move {
                    super::$name(&calendar).await
                })
                .await;
            }

            #[tokio::test]
            #[ignore = "本物のテスト用カレンダーが要る（モジュールの説明を参照）"]
            async fn with_a_real_calendar() {
                run(Calendar::real().await, |calendar| async move {
                    super::$name(&calendar).await
                })
                .await;
            }
        }
    };
}

contract!(conditional_writes);
contract!(a_recurring_event_is_listed_only_around_its_occurrences);
contract!(one_occurrence_is_written_by_its_instance_id);
contract!(a_one_time_reservation_round_trips);
contract!(a_recurring_reservation_round_trips);
contract!(an_event_that_does_not_exist_is_not_found);
contract!(an_id_that_was_used_cannot_be_used_again);
contract!(listing_leaves_out_events_that_only_touch_the_period);
contract!(a_moved_occurrence_alone_in_the_period_is_found);
contract!(a_weekly_event_keeps_its_local_time_across_daylight_saving_time);
