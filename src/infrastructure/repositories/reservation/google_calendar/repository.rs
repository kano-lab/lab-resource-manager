use super::event_gateway::{CalendarEventGateway, GoogleCalendarEventGateway};
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::reservation::{
    Exception, ExceptionChange, RecurrenceRule, RecurringSchedule, Reservation, ReservationId,
    ReservationStatus, Schedule, Version, factory::ResourceFactory, value_objects::Resource,
};
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, RepositoryError, ReservationRepository, ReservedSlotQuery,
};
use crate::domain::read_models::ReservedSlotView;
use crate::infrastructure::config::{ResourceConfig, StorageConfig};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use google_calendar3::{
    CalendarHub,
    api::{Event, EventDateTime},
    hyper_rustls::HttpsConnectorBuilder,
    hyper_util::{client::legacy::Client, rt::TokioExecutor},
    yup_oauth2,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

/// Google Calendar の外部ID (calendar_id + event_id)
#[derive(Debug, Clone)]
struct ExternalId {
    /// Calendar ID
    calendar_id: String,
    /// Event ID
    event_id: String,
}

/// アプリ経由で作成されたイベントのdescriptionに付与される、予約者メールアドレス行の接頭辞
const HOLDER_LINE_PREFIX: &str = "予約者: ";

/// アプリが管理するセクションの開始マーカー
///
/// ユーザーが入力しうる自然文と衝突しないよう、アプリ名を含む機械識別用の符号を用いる。
const MANAGED_SECTION_BEGIN: &str = "[lab-resource-manager:managed-section:begin]";

/// アプリが管理するセクションの終了マーカー。このマーカーより後ろが備考として扱われる。
const MANAGED_SECTION_END: &str = "[lab-resource-manager:managed-section:end]";

/// 予約者のOSユーザー名行の接頭辞
const OS_USER_LINE_PREFIX: &str = "OS: ";

/// 予約IDの行の接頭辞
const RESERVATION_ID_LINE_PREFIX: &str = "予約ID: ";

/// 予約IDからカレンダーのイベントIDを導出する
///
/// イベントIDに使える文字はbase32hex（英小文字a-vと数字0-9）に限られるため、
/// UUIDのハイフンを除いた表現を用いる。予約IDとイベントIDが決定的に対応するので、
/// 両者の対応表を持たなくても相互に解決できる。
///
/// v2.0.0 以降はすべての予約のイベントIDを予約IDから導出可能。
fn event_id_for(reservation_id: &ReservationId) -> String {
    reservation_id.as_str().replace('-', "")
}

/// 繰り返しイベントの個々の回のイベントIDで、回の本来の開始時刻を書く形式（UTC）
const INSTANCE_START_FORMAT: &str = "%Y%m%dT%H%M%SZ";

/// 繰り返しイベントの、本来`start`に始まる回のイベントID
fn instance_event_id(series_event_id: &str, start: DateTime<Utc>) -> String {
    format!("{series_event_id}_{}", start.format(INSTANCE_START_FORMAT))
}

/// 取得したイベントと、それが繰り返しイベントなら規則どおりでない回のイベント
struct FetchedEvent {
    event: Event,
    exceptions: Vec<Event>,
    calendar_id: String,
    resource_context: String,
}

/// キャンセル済みイベントのstatus値
const EVENT_STATUS_CANCELLED: &str = "cancelled";

/// イベントが有効な（キャンセルされていない）予約かどうか
///
/// 繰り返しイベントを個々の予定に展開して取得すると、削除された回が
/// `status: cancelled`として返ることがある。これは予約として扱わない。
fn is_active_event(event: &Event) -> bool {
    event.status.as_deref() != Some(EVENT_STATUS_CANCELLED)
}

/// Google Calendar APIを使用した予約リポジトリ実装
pub struct GoogleCalendarReservationRepository {
    gateway: Arc<dyn CalendarEventGateway>,
    config: ResourceConfig,
    storage_config: StorageConfig,
    service_account_email: String,
    /// 予約者の付加情報（OSユーザー名など）を引くためのリポジトリ
    identity_repo: Arc<dyn IdentityLinkRepository>,
    /// 解釈失敗を既に警告したイベントID
    ///
    /// 同じイベントは取得のたびに失敗するため、警告を出し続けるとログが埋まる。
    reported_parse_failures: Mutex<HashSet<String>>,
}

impl GoogleCalendarReservationRepository {
    /// 新しいGoogle Calendarリポジトリを作成
    ///
    /// # Arguments
    /// * `service_account_key` - サービスアカウントキーファイルのパス
    /// * `config` - リソース設定
    /// * `identity_repo` - 同一性リンクリポジトリ
    /// * `storage_config` - ストレージバックエンド設定（カレンダーIDマッピング等）
    pub async fn new(
        service_account_key: &str,
        config: ResourceConfig,
        identity_repo: Arc<dyn IdentityLinkRepository>,
        storage_config: StorageConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let secret = yup_oauth2::read_service_account_key(service_account_key).await?;
        let service_account_email = secret.client_email.clone();

        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret)
            .build()
            .await?;

        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(connector);

        let hub = CalendarHub::new(client, auth);

        Ok(Self {
            gateway: Arc::new(GoogleCalendarEventGateway::new(hub)),
            config,
            storage_config,
            service_account_email,
            identity_repo,
            reported_parse_failures: Mutex::new(HashSet::new()),
        })
    }

    /// ゲートウェイを差し替えてリポジトリを構築する（テスト用）
    #[cfg(test)]
    pub(super) fn with_gateway(
        gateway: Arc<dyn CalendarEventGateway>,
        config: ResourceConfig,
        service_account_email: String,
        identity_repo: Arc<dyn IdentityLinkRepository>,
        storage_config: StorageConfig,
    ) -> Result<Self, RepositoryError> {
        Ok(Self {
            gateway,
            config,
            storage_config,
            service_account_email,
            identity_repo,
            reported_parse_failures: Mutex::new(HashSet::new()),
        })
    }

    /// イベントのdescriptionを組み立てる
    ///
    /// アプリが管理する行はマーカーで囲む。カレンダーを開いた人が状況を把握できるよう、
    /// 予約者のメールアドレスに加えて、分かる範囲の情報（OSユーザー名・予約ID）を並べる。
    /// 備考はマーカーの外に置き、利用者が自由に編集できる領域として残す。
    #[cfg(test)]
    pub(super) async fn build_description(&self, reservation: &Reservation) -> String {
        self.build_description_with_notes(reservation, reservation.notes())
            .await
    }

    /// 備考を指定してイベントのdescriptionを組み立てる（定期予約の回は回ごとの備考を持つ）
    async fn build_description_with_notes(
        &self,
        reservation: &Reservation,
        notes: Option<&String>,
    ) -> String {
        let mut lines = vec![
            MANAGED_SECTION_BEGIN.to_string(),
            format!(
                "{HOLDER_LINE_PREFIX}{}",
                reservation.holder_email().as_str()
            ),
        ];

        if let Some(os_user) = self.resolve_os_user_name(reservation).await {
            lines.push(format!("{OS_USER_LINE_PREFIX}{os_user}"));
        }

        lines.push(format!(
            "{RESERVATION_ID_LINE_PREFIX}{}",
            reservation.id().as_str()
        ));
        lines.push(MANAGED_SECTION_END.to_string());

        let mut description = lines.join("\n");

        if let Some(notes) = notes {
            description.push_str(&format!("\n\n{notes}"));
        }

        description
    }

    /// 予約対象サーバーにおける予約者のOSユーザー名を引く
    ///
    /// OSユーザー名の名前空間はサーバーごとに異なりうるため、予約したサーバーの紐付けを見る。
    /// 部屋の予約や紐付けが無い場合はNoneを返す。表示のための情報であり、
    /// 引けないことを理由に予約の保存を失敗させない。
    async fn resolve_os_user_name(&self, reservation: &Reservation) -> Option<String> {
        let server = reservation
            .resources()
            .iter()
            .find_map(|resource| match resource {
                Resource::Gpu(gpu) => Some(gpu.server().to_string()),
                Resource::Room { .. } => None,
            })?;

        let identity_link = self
            .identity_repo
            .find_by_email(reservation.holder_email())
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    holder = %reservation.holder_email().as_str(),
                    error = %e,
                    "looking up the reserver's identity link failed; omitting the os user name"
                )
            })
            .ok()
            .flatten()?;

        identity_link
            .get_identity_for_system(&ExternalSystem::Os { server })
            .map(|identity| identity.user_id().to_string())
    }

    /// 管理対象のカレンダー一覧
    /// 戻り値: (calendar_id, resource_name)
    fn calendars(&self) -> Vec<(String, String)> {
        let mappings = match self.storage_config.google_calendar_mappings() {
            Some(m) => m,
            None => return Vec::new(),
        };

        let mut calendars = Vec::new();

        // サーバーのカレンダーを追加
        for server in &self.config.servers {
            if let Some(calendar_id) = mappings.get(&server.name) {
                calendars.push((calendar_id.clone(), server.name.clone()));
            }
        }

        // 部屋のカレンダーを追加
        for room in &self.config.rooms {
            if let Some(calendar_id) = mappings.get(&room.name) {
                calendars.push((calendar_id.clone(), room.name.clone()));
            }
        }

        calendars
    }

    /// すべてのカレンダーから、指定期間に重なるイベントを取得
    ///
    /// `time_min`は「イベントの終了時刻がこれより後」、`time_max`は「イベントの開始時刻が
    /// これより前」を意味する。どの期間を対象とするかは呼び出し側が決める。
    ///
    /// 繰り返しイベントは、規則どおりでない回のイベントをすべて添えて返す。期間内に
    /// 時間を変えた回だけがあり、繰り返しイベントそのものが一覧に無い場合も拾う。
    /// 削除されたイベントと、繰り返しイベントの回だけのイベントは、予約として返さない。
    async fn fetch_events(
        &self,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<FetchedEvent>, RepositoryError> {
        let mut fetched = Vec::new();

        for (calendar_id, resource_name) in self.calendars() {
            let events = self
                .gateway
                .list_events(&calendar_id, time_min, time_max)
                .await?;

            let mut series = Vec::new();
            let mut series_of_instances = Vec::new();
            for event in events {
                if let Some(series_id) = &event.recurring_event_id {
                    series_of_instances.push(series_id.clone());
                } else if is_active_event(&event) {
                    series.push(event);
                }
            }
            series_of_instances.sort();
            series_of_instances.dedup();
            for series_id in series_of_instances {
                let listed = series
                    .iter()
                    .any(|event| event.id.as_deref() == Some(series_id.as_str()));
                if listed {
                    continue;
                }
                if let Some(event) = self.gateway.get_event(&calendar_id, &series_id).await?
                    && is_active_event(&event)
                {
                    series.push(event);
                }
            }

            for event in series {
                fetched.push(
                    self.with_exceptions(event, &calendar_id, &resource_name)
                        .await?,
                );
            }
        }

        Ok(fetched)
    }

    /// イベントに、それが繰り返しイベントなら規則どおりでない回のイベントを添える
    async fn with_exceptions(
        &self,
        event: Event,
        calendar_id: &str,
        resource_context: &str,
    ) -> Result<FetchedEvent, RepositoryError> {
        let exceptions = match (&event.recurrence, &event.i_cal_uid, &event.id) {
            (Some(_), Some(ical_uid), Some(series_id)) => self
                .gateway
                .list_series(calendar_id, ical_uid)
                .await?
                .into_iter()
                .filter(|other| other.recurring_event_id.as_deref() == Some(series_id.as_str()))
                .collect(),
            _ => Vec::new(),
        };
        Ok(FetchedEvent {
            event,
            exceptions,
            calendar_id: calendar_id.to_string(),
            resource_context: resource_context.to_string(),
        })
    }

    /// 解釈失敗を警告として報告すべきか（イベントごとに初回のみ）
    pub(super) fn should_report_parse_failure(&self, event_id: &str) -> bool {
        self.reported_parse_failures
            .lock()
            .expect("解釈失敗の記録ロックを取得できませんでした")
            .insert(event_id.to_string())
    }

    /// 取得したイベント群を予約へ変換する
    ///
    /// 予約として解釈できないイベント（カレンダー上で直接作られた自由記述のイベントなど）は
    /// 警告を残して飛ばす。1件の解釈失敗で予約機能全体を止めないための判断。
    fn parse_events(&self, events: Vec<FetchedEvent>) -> Vec<Reservation> {
        events
            .into_iter()
            .filter_map(|fetched| {
                let FetchedEvent {
                    event,
                    exceptions,
                    calendar_id,
                    resource_context,
                } = fetched;
                let event_id = event.id.clone().unwrap_or_default();
                match self.parse_event(event, &exceptions, &resource_context) {
                    Ok(reservation) => Some(reservation),
                    Err(e) => {
                        if self.should_report_parse_failure(&event_id) {
                            tracing::warn!(
                                calendar_id = %calendar_id,
                                event_id = %event_id,
                                error = %e,
                                "the event is not a reservation; skipping it"
                            );
                        } else {
                            tracing::debug!(
                                event_id = %event_id,
                                "the event is not a reservation; skipping it (already reported)"
                            );
                        }
                        None
                    }
                }
            })
            .collect()
    }

    /// イベントを予約に変換
    ///
    /// 繰り返しイベントは、規則どおりでない回のイベント（`exceptions`）とあわせて定期予約に、
    /// それ以外は単発予約にする。イベントのetagを予約の版とする。カレンダーはイベントを
    /// 書き換えるたびにetagを変える。
    fn parse_event(
        &self,
        event: Event,
        exceptions: &[Event],
        resource_context: &str,
    ) -> Result<Reservation, RepositoryError> {
        // Event ID から Domain ID を取得
        let event_id = event.id.clone().unwrap_or_default();

        // イベントIDがそのまま予約IDになる
        let id = ReservationId::from_string(event_id);
        let version = event.etag.clone().map(Version::new);

        // holder_emailの決定ロジック
        let holder_email = event
            .creator
            .as_ref()
            .and_then(|c| c.email.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("作成者情報がありません".to_string()))?;

        // creatorがサービスアカウントの場合はdescriptionから実際のユーザーを取得
        let holder_email = if holder_email == &self.service_account_email {
            event
                .description
                .as_ref()
                .and_then(|desc| {
                    // "予約者: user@example.com" 行をアプリ管理セクション内から探して抽出
                    desc.lines()
                        .find_map(|line| line.strip_prefix(HOLDER_LINE_PREFIX))
                })
                .ok_or_else(|| {
                    RepositoryError::Unknown(
                        "サービスアカウントで作成されたイベントのdescriptionにユーザー情報がありません"
                            .to_string(),
                    )
                })?
        } else {
            holder_email
        };

        let user = self.parse_user(holder_email)?;

        let time_period = Self::period_of(&event)?;

        // タイトルから資源をパース
        let default_title = String::new();
        let title = event.summary.as_ref().unwrap_or(&default_title);
        let items = self.parse_resources(title, resource_context)?;

        let notes = event
            .description
            .as_ref()
            .and_then(|desc| Self::extract_notes(desc));

        let schedule = match &event.recurrence {
            Some(lines) => Schedule::Recurring(Self::parse_recurring_schedule(
                &event,
                lines,
                time_period,
                exceptions,
            )?),
            None => Schedule::OneTime(time_period),
        };

        Reservation::reconstruct(id, user, items, notes, schedule, version)
            .map_err(RepositoryError::from)
    }

    /// イベントの期間
    fn period_of(event: &Event) -> Result<TimePeriod, RepositoryError> {
        let start = event
            .start
            .as_ref()
            .and_then(|s| s.date_time.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("開始時刻がありません".to_string()))?;

        let end = event
            .end
            .as_ref()
            .and_then(|e| e.date_time.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("終了時刻がありません".to_string()))?;

        TimePeriod::new(*start, *end)
            .map_err(|e| RepositoryError::Unknown(format!("時間枠エラー: {}", e)))
    }

    /// 繰り返しイベントと、その規則どおりでない回のイベントから、定期予約の日程を組み立てる
    ///
    /// 回の時刻は、繰り返しイベントの開始時刻の時間帯で数える（指定が無ければUTC）。
    /// 回のイベントからは、期間と備考と版を読む。削除された回は取り消した回になる。
    fn parse_recurring_schedule(
        event: &Event,
        lines: &[String],
        first: TimePeriod,
        exceptions: &[Event],
    ) -> Result<RecurringSchedule, RepositoryError> {
        let rule = RecurrenceRule::parse(lines.to_vec())
            .map_err(|e| RepositoryError::Unknown(e.to_string()))?;
        let time_zone = event
            .start
            .as_ref()
            .and_then(|start| start.time_zone.as_deref())
            .and_then(|name| name.parse::<Tz>().ok())
            .unwrap_or(Tz::UTC);

        let mut by_start = BTreeMap::new();
        for exception in exceptions {
            let Some(original_start) = exception
                .original_start_time
                .as_ref()
                .and_then(|original| original.date_time)
            else {
                continue;
            };
            let change = if is_active_event(exception) {
                let Ok(time_period) = Self::period_of(exception) else {
                    continue;
                };
                ExceptionChange::Changed {
                    time_period,
                    notes: exception
                        .description
                        .as_ref()
                        .and_then(|desc| Self::extract_notes(desc)),
                }
            } else {
                ExceptionChange::Cancelled
            };
            by_start.insert(
                original_start,
                Exception::new(change, exception.etag.clone().map(Version::new)),
            );
        }

        Ok(RecurringSchedule::new(first, rule, time_zone, by_start))
    }

    /// descriptionから備考（アプリ管理セクションの外側）を取り出す
    ///
    /// ユーザーが開始マーカーより前にメモを追記した場合も失わないよう、
    /// マーカーの前後両方を拾って結合する。
    /// 旧形式（"予約者: xxx"の1行目のみでマーカー無し）は先頭行のみ除外し、
    /// Googleカレンダー上で直接作成されたイベント（マーカーも予約者行も無し）は
    /// description全体をそのまま備考として扱う。
    pub(super) fn extract_notes(description: &str) -> Option<String> {
        let body: String = if let Some((before_begin, after_begin)) =
            description.split_once(MANAGED_SECTION_BEGIN)
        {
            match after_begin.split_once(MANAGED_SECTION_END) {
                Some((_, after_end)) => {
                    let before = before_begin.trim();
                    let after = after_end.trim();
                    if before.is_empty() {
                        after.to_string()
                    } else if after.is_empty() {
                        before.to_string()
                    } else {
                        format!("{before}\n\n{after}")
                    }
                }
                // 開始マーカーのみで終了マーカーが無い想定外の編集は、
                // 安全側に倒してdescription全体を備考として扱う
                None => description.to_string(),
            }
        } else if description.starts_with(HOLDER_LINE_PREFIX) {
            description
                .split_once('\n')
                .map(|(_, rest)| rest.to_string())
                .unwrap_or_default()
        } else {
            description.to_string()
        };

        let body = body.trim();
        if body.is_empty() {
            None
        } else {
            Some(body.to_string())
        }
    }

    /// メールアドレスからEmailAddressを作成
    fn parse_user(&self, email: &str) -> Result<EmailAddress, RepositoryError> {
        EmailAddress::new(email.to_string()).map_err(RepositoryError::from)
    }

    /// タイトルから資源をパース
    fn parse_resources(
        &self,
        title: &str,
        resource_context: &str,
    ) -> Result<Vec<Resource>, RepositoryError> {
        // 部屋の場合
        if let Some(room) = self
            .config
            .rooms
            .iter()
            .find(|r| r.name == resource_context)
        {
            return Ok(vec![Resource::Room {
                name: room.name.clone(),
            }]);
        }

        // GPU（サーバー）の場合: ResourceFactoryを使用
        let server = self.config.get_server(resource_context).ok_or_else(|| {
            RepositoryError::Unknown(format!("サーバーが見つかりません: {}", resource_context))
        })?;

        ResourceFactory::create_gpus_from_spec(title, &server.name, |device_id| {
            server
                .devices
                .iter()
                .find(|d| d.id == device_id)
                .map(|d| d.model.clone())
        })
        .map_err(|e| RepositoryError::Unknown(e.to_string()))
    }

    /// ResourcesからGPUデバイス仕様文字列を生成
    ///
    /// GPUリソースからデバイス番号を抽出してソートし、
    /// カンマ区切りの文字列として返す（例: "0,1,5,7"）
    fn format_gpu_spec(&self, resources: &[Resource]) -> Option<String> {
        let mut device_numbers: Vec<u32> = resources
            .iter()
            .filter_map(|r| match r {
                Resource::Gpu(gpu) => Some(gpu.device_number()),
                _ => None,
            })
            .collect();

        if device_numbers.is_empty() {
            return None;
        }

        device_numbers.sort_unstable();

        Some(
            device_numbers
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    /// 保存対象の予約が既にカレンダー上に存在するかを調べる
    ///
    /// 予約IDとイベントIDは決定的に対応するため、対応表が無くてもイベントIDで確認できる。
    /// 削除済み（`status: cancelled`）のイベントは無いものとして扱う。
    async fn locate_existing_event(
        &self,
        reservation_id: &ReservationId,
        target_calendar_id: &str,
    ) -> Result<Option<ExternalId>, RepositoryError> {
        let event_id = event_id_for(reservation_id);

        // 配置先を先に確認し、無ければ他のカレンダー（移動元）も探す
        if self
            .gateway
            .get_event(target_calendar_id, &event_id)
            .await?
            .is_some_and(|event| is_active_event(&event))
        {
            return Ok(Some(ExternalId {
                calendar_id: target_calendar_id.to_string(),
                event_id,
            }));
        }

        for (calendar_id, _resource_name) in self.calendars() {
            if calendar_id == target_calendar_id {
                continue;
            }
            if self
                .gateway
                .get_event(&calendar_id, &event_id)
                .await?
                .is_some_and(|event| is_active_event(&event))
            {
                return Ok(Some(ExternalId {
                    calendar_id,
                    event_id,
                }));
            }
        }

        Ok(None)
    }

    /// 予約を、呼び出し側が指定したIDで返す
    ///
    /// イベントIDと予約IDは表記が異なりうる（ハイフンの有無）。
    /// IDを指定して取得した予約は、必ずそのIDで返す必要がある。
    fn with_id(
        reservation: Reservation,
        id: &ReservationId,
    ) -> Result<Reservation, RepositoryError> {
        if reservation.id() == id {
            return Ok(reservation);
        }

        Reservation::reconstruct(
            id.clone(),
            reservation.holder_email().clone(),
            reservation.resources().to_vec(),
            reservation.notes().cloned(),
            reservation.schedule().clone(),
            reservation.version().cloned(),
        )
        .map_err(RepositoryError::from)
    }

    /// 予約IDから導出したイベントIDを指定して、カレンダーにイベントを作成し、作成したイベントを返す
    async fn insert_event_for(
        &self,
        reservation: &Reservation,
        calendar_id: &str,
    ) -> Result<Event, RepositoryError> {
        let mut event = self.create_event_from_reservation(reservation).await?;
        event.id = Some(event_id_for(reservation.id()));

        self.gateway.insert_event(calendar_id, event).await
    }

    /// 予約から適切なカレンダーIDを取得
    ///
    /// # 前提条件
    /// このメソッドは、予約のすべてのリソースが同一のカレンダーに属することを前提としています。
    /// （例: すべてGPU、またはすべて部屋）
    /// 混在している場合はエラーを返します。
    fn get_calendar_id_for_reservation(
        &self,
        reservation: &Reservation,
    ) -> Result<String, RepositoryError> {
        let resources = reservation.resources();
        if resources.is_empty() {
            return Err(RepositoryError::Unknown("リソースが空です".to_string()));
        }

        // すべてのリソースが同じタイプ（GPU or Room）であることを検証
        let first_resource = &resources[0];
        let all_same_type = match first_resource {
            Resource::Gpu(first_gpu) => {
                // すべてのリソースがGPUで、同じサーバーに属することを確認
                let server_name = first_gpu.server();
                resources
                    .iter()
                    .all(|r| matches!(r, Resource::Gpu(gpu) if gpu.server() == server_name))
            }
            Resource::Room { name: first_name } => {
                // すべてのリソースが同じ部屋であることを確認
                resources
                    .iter()
                    .all(|r| matches!(r, Resource::Room { name } if name == first_name))
            }
        };

        if !all_same_type {
            return Err(RepositoryError::Unknown(
                "複数の異なるリソースタイプまたは異なるカレンダーに属するリソースが混在しています"
                    .to_string(),
            ));
        }

        match first_resource {
            Resource::Gpu(gpu) => {
                // load_config で検証済みだが、念のため確認（防御的プログラミング）
                let _ = self.config.get_server(gpu.server()).ok_or_else(|| {
                    RepositoryError::Unknown(format!("サーバーが見つかりません: {}", gpu.server()))
                })?;
                self.storage_config
                    .calendar_id_for_resource(gpu.server())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        RepositoryError::Unknown(format!(
                            "カレンダーIDマッピングが見つかりません: {}",
                            gpu.server()
                        ))
                    })
            }
            Resource::Room { name } => {
                // load_config で検証済みだが、念のため確認（防御的プログラミング）
                let _ = self
                    .config
                    .rooms
                    .iter()
                    .find(|r| &r.name == name)
                    .ok_or_else(|| {
                        RepositoryError::Unknown(format!("部屋が見つかりません: {}", name))
                    })?;
                self.storage_config
                    .calendar_id_for_resource(name)
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        RepositoryError::Unknown(format!(
                            "カレンダーIDマッピングが見つかりません: {}",
                            name
                        ))
                    })
            }
        }
    }

    /// 予約をGoogle Calendar Eventに変換
    ///
    /// # 前提条件
    /// このメソッドは、get_calendar_id_for_reservationで検証済みの予約を受け取ることを前提としています。
    /// すなわち、すべてのリソースが同一のカレンダーに属していることが保証されています。
    async fn create_event_from_reservation(
        &self,
        reservation: &Reservation,
    ) -> Result<Event, RepositoryError> {
        let time_period = match reservation.schedule() {
            Schedule::OneTime(time_period) => time_period,
            Schedule::Recurring(_) => {
                return Err(RepositoryError::Unknown(
                    "定期予約そのものはアプリから作成・変更できません".to_string(),
                ));
            }
        };
        self.event_for(reservation, time_period, reservation.notes())
            .await
    }

    /// 予約の、指定した期間と備考のイベントを組み立てる
    ///
    /// 単発予約のイベントにも、定期予約の1つの回のイベントにも使う。
    async fn event_for(
        &self,
        reservation: &Reservation,
        time_period: &TimePeriod,
        notes: Option<&String>,
    ) -> Result<Event, RepositoryError> {
        // 注: get_calendar_id_for_reservationで検証済みのため、resources()[0]は安全に使用できる
        let summary = match &reservation.resources()[0] {
            Resource::Gpu(_) => self
                .format_gpu_spec(reservation.resources())
                .ok_or_else(|| {
                    RepositoryError::Unknown("GPUデバイス仕様の生成に失敗しました".to_string())
                })?,
            Resource::Room { name } => name.clone(),
        };

        let description = self.build_description_with_notes(reservation, notes).await;

        Ok(Event {
            summary: Some(summary),
            description: Some(description),
            start: Some(EventDateTime {
                date_time: Some(time_period.start()),
                ..Default::default()
            }),
            end: Some(EventDateTime {
                date_time: Some(time_period.end()),
                ..Default::default()
            }),
            // NOTE: attendeesを追加するとDomain-Wide Delegationが必要になるため、
            // 予約者情報はdescriptionに含めています
            ..Default::default()
        })
    }

    /// event_idから直接イベントを検索（マッピングがない場合）
    ///
    /// 全カレンダーから該当するイベントを検索して予約を返します。
    /// 削除済み（`status: cancelled`）のイベントと、繰り返しイベントの1つの回のイベントは
    /// 予約ではないため、見つからなかったものとします。
    async fn find_by_event_id(
        &self,
        event_id: &str,
    ) -> Result<Option<Reservation>, RepositoryError> {
        // 各カレンダーでイベントの検索を試みる
        for (calendar_id, resource_context) in self.calendars() {
            match self.gateway.get_event(&calendar_id, event_id).await? {
                Some(event) if is_active_event(&event) && event.recurring_event_id.is_none() => {
                    let fetched = self
                        .with_exceptions(event, &calendar_id, &resource_context)
                        .await?;
                    let reservation = self.parse_event(
                        fetched.event,
                        &fetched.exceptions,
                        &fetched.resource_context,
                    )?;
                    return Ok(Some(reservation));
                }
                // 繰り返しイベントの1つの回は、予約ではない
                Some(event) if event.recurring_event_id.is_some() => return Ok(None),
                // 削除済みなら、別のカレンダーへ移った同じIDのイベントがあるかもしれない
                _ => {
                    // 次のカレンダーを試す
                    continue;
                }
            }
        }

        // すべてのカレンダーで見つからなかった
        Ok(None)
    }

    /// 定期予約の、読んだあとに変えた回を書く
    ///
    /// 繰り返しイベントそのものはアプリから変えないので、書くのは回のイベントだけ。
    /// 回ごとに、読んだときの版のままかを確かめてから書く（`If-Match`）。まだ規則どおり
    /// だった回は、繰り返しイベントの版が読んだときのままで、その回がまだ規則どおりの
    /// ときだけ書く。どれか1つでも確かめられなければ、何も書かずに
    /// `RepositoryError::Conflict`を返す。
    async fn save_occurrences(
        &self,
        reservation: &mut Reservation,
        version: &Version,
    ) -> Result<(), RepositoryError> {
        let Schedule::Recurring(recurring) = reservation.schedule() else {
            return Ok(());
        };
        let changes: Vec<(DateTime<Utc>, ExceptionChange, Option<Version>)> = recurring
            .unsaved_occurrences()
            .iter()
            .filter_map(|start| {
                recurring.exceptions().get(start).map(|exception| {
                    (
                        *start,
                        exception.change().clone(),
                        exception.version().cloned(),
                    )
                })
            })
            .collect();
        if changes.is_empty() {
            return Ok(());
        }
        let time_zone = recurring.time_zone();

        // 先にすべて確かめる。書いている途中で断ると、一部の回だけが書かれる
        let stored = self
            .find(reservation.id())
            .await?
            .ok_or(RepositoryError::Conflict)?;
        let Schedule::Recurring(stored_schedule) = stored.schedule() else {
            return Err(RepositoryError::Conflict);
        };
        if stored.version() != Some(version) {
            return Err(RepositoryError::Conflict);
        }
        for (start, _, read_at) in &changes {
            let stored_at = stored_schedule
                .exceptions()
                .get(start)
                .and_then(Exception::version);
            if read_at.as_ref() != stored_at {
                return Err(RepositoryError::Conflict);
            }
        }

        let calendar_id = self.get_calendar_id_for_reservation(reservation)?;
        let series = self
            .locate_existing_event(reservation.id(), &calendar_id)
            .await?
            .ok_or(RepositoryError::Conflict)?;

        for (start, change, read_at) in changes {
            let instance_id = instance_event_id(&series.event_id, start);
            // まだ規則どおりだった回は、今のその回の版を条件にする
            let precondition = match read_at {
                Some(read_at) => read_at,
                None => self
                    .gateway
                    .get_event(&series.calendar_id, &instance_id)
                    .await?
                    .and_then(|instance| instance.etag)
                    .map(Version::new)
                    .ok_or(RepositoryError::Conflict)?,
            };

            let saved_version = match change {
                ExceptionChange::Changed { time_period, notes } => {
                    let mut event = self
                        .event_for(reservation, &time_period, notes.as_ref())
                        .await?;
                    event.id = Some(instance_id.clone());
                    event.recurring_event_id = Some(series.event_id.clone());
                    event.original_start_time = Some(EventDateTime {
                        date_time: Some(start),
                        time_zone: Some(time_zone.name().to_string()),
                        ..Default::default()
                    });
                    self.gateway
                        .update_event(
                            &series.calendar_id,
                            &instance_id,
                            event,
                            precondition.as_str(),
                        )
                        .await?
                        .etag
                        .map(Version::new)
                }
                ExceptionChange::Cancelled => {
                    self.gateway
                        .delete_event(&series.calendar_id, &instance_id, precondition.as_str())
                        .await?;
                    None
                }
            };
            reservation.record_occurrence_version(start, saved_version);

            tracing::info!(
                reservation_id = %reservation.id().as_str(),
                occurrence = %start,
                calendar_id = %series.calendar_id,
                "occurrence of a recurring reservation saved to the calendar"
            );
        }
        Ok(())
    }
}

#[async_trait]
impl ReservedSlotQuery for GoogleCalendarReservationRepository {
    /// 指定期間と重複する予約枠を検索
    ///
    /// 対象期間そのものをカレンダーに問い合わせる。終了済みかどうかで絞ってはいけない。
    /// 事後予約のように過去の期間を予約する場合、相手の予約が既に終了していても
    /// 競合は競合であり、終了済みを除外すると重複予約を許してしまう。
    async fn find_slots_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
        Ok(self
            .find_overlapping(time_period)
            .await?
            .iter()
            .flat_map(|reservation| {
                reservation
                    .slots_in(time_period)
                    .iter()
                    .map(|slot| ReservedSlotView::of(reservation, slot))
                    .collect::<Vec<_>>()
            })
            .collect())
    }

    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
        Ok(self
            .find_slots_overlapping(time_period)
            .await?
            .into_iter()
            .filter(|slot| slot.holder_email() == holder_email)
            .collect())
    }
}

#[async_trait]
impl ReservationRepository for GoogleCalendarReservationRepository {
    async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError> {
        // 予約IDから導出したイベントIDで全カレンダーを探す
        let found = self.find_by_event_id(&event_id_for(id)).await?;
        found
            .map(|reservation| Self::with_id(reservation, id))
            .transpose()
    }

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<Reservation>, RepositoryError> {
        let events = self
            .fetch_events(time_period.start(), Some(time_period.end()))
            .await?;

        Ok(self
            .parse_events(events)
            .into_iter()
            .filter(|reservation| !reservation.slots_in(time_period).is_empty())
            .collect())
    }

    /// 予約を、状態と版に応じてカレンダーへ書く
    ///
    /// 版のある書き込み（書き換え・削除）は、イベントのetagが版のままのときだけ通す
    /// （`If-Match`）。読んだあとに誰かがカレンダーで書き換えた・消した予約は
    /// `RepositoryError::Conflict`になり、他人の変更を上書きしない。
    async fn save(&self, reservation: &mut Reservation) -> Result<(), RepositoryError> {
        match (reservation.status(), reservation.version().cloned()) {
            (ReservationStatus::Active, None) => {
                let calendar_id = self.get_calendar_id_for_reservation(reservation)?;
                let created = self.insert_event_for(reservation, &calendar_id).await?;
                reservation.record_version(created.etag.map(Version::new));

                tracing::info!(
                    reservation_id = %reservation.id().as_str(),
                    calendar_id = %calendar_id,
                    "reservation added to the calendar"
                );
            }
            (ReservationStatus::Active, Some(version)) if reservation.is_recurring() => {
                self.save_occurrences(reservation, &version).await?;
            }
            (ReservationStatus::Active, Some(version)) => {
                let new_calendar_id = self.get_calendar_id_for_reservation(reservation)?;
                let external_id = self
                    .locate_existing_event(reservation.id(), &new_calendar_id)
                    .await?
                    .ok_or(RepositoryError::Conflict)?;

                let saved = if external_id.calendar_id == new_calendar_id {
                    // 同じカレンダー → 版が変わっていなければ書き換える
                    // IMPORTANT: update API用に id フィールドを含む Event を作成
                    let mut event = self.create_event_from_reservation(reservation).await?;
                    event.id = Some(external_id.event_id.clone());

                    self.gateway
                        .update_event(
                            &external_id.calendar_id,
                            &external_id.event_id,
                            event,
                            version.as_str(),
                        )
                        .await?
                } else {
                    // カレンダーが変更された → 版が変わっていなければ古いカレンダーから削除し、
                    // 新しいカレンダーに作成する
                    self.gateway
                        .delete_event(
                            &external_id.calendar_id,
                            &external_id.event_id,
                            version.as_str(),
                        )
                        .await?;

                    // 移動先でも同じイベントIDを使うため、対応表は更新しなくてよい
                    self.insert_event_for(reservation, &new_calendar_id)
                        .await
                        .map_err(|e| {
                            RepositoryError::ConnectionError(format!(
                                "新しいカレンダーへのイベント作成に失敗: {}",
                                e
                            ))
                        })?
                };
                reservation.record_version(saved.etag.map(Version::new));
            }
            (ReservationStatus::Cancelled, Some(version)) => {
                let calendar_id = self.get_calendar_id_for_reservation(reservation)?;
                let external_id = self
                    .locate_existing_event(reservation.id(), &calendar_id)
                    .await?
                    .ok_or(RepositoryError::Conflict)?;
                self.gateway
                    .delete_event(
                        &external_id.calendar_id,
                        &external_id.event_id,
                        version.as_str(),
                    )
                    .await?;

                tracing::info!(
                    reservation_id = %reservation.id().as_str(),
                    calendar_id = %external_id.calendar_id,
                    "reservation removed from the calendar"
                );
            }
            // 保存していないまま取り消した予約は、カレンダーに書くものが無い
            (ReservationStatus::Cancelled, None) => {}
        }
        Ok(())
    }
}
