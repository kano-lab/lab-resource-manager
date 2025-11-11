use crate::domain::aggregates::resource_usage::{
    entity::ResourceUsage,
    factory::ResourceFactory,
    value_objects::{Resource, TimePeriod, UsageId},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use crate::infrastructure::config::ResourceConfig;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use google_calendar3::{
    CalendarHub,
    api::Event,
    hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
    hyper_util::{
        client::legacy::{Client, connect::HttpConnector},
        rt::TokioExecutor,
    },
    yup_oauth2,
};

/// Google Calendar APIを使用したResourceUsageリポジトリ実装
pub struct GoogleCalendarUsageRepository {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
    config: ResourceConfig,
    service_account_email: String,
}

impl GoogleCalendarUsageRepository {
    /// 新しいGoogle Calendarリポジトリを作成
    ///
    /// # Arguments
    /// * `service_account_key` - サービスアカウントキーファイルのパス
    /// * `config` - リソース設定
    pub async fn new(
        service_account_key: &str,
        config: ResourceConfig,
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
            hub,
            config,
            service_account_email,
        })
    }

    /// すべてのカレンダーから未来のイベントを取得
    async fn fetch_future_events(&self) -> Result<Vec<(Event, String)>, RepositoryError> {
        let mut all_events = Vec::new();

        // 各サーバーカレンダーから取得
        for server in &self.config.servers {
            let events = self.fetch_events_from_calendar(&server.calendar_id).await?;
            all_events.extend(events.into_iter().map(|e| (e, server.name.clone())));
        }

        // 部屋カレンダーから取得
        for room in &self.config.rooms {
            let events = self.fetch_events_from_calendar(&room.calendar_id).await?;
            all_events.extend(events.into_iter().map(|e| (e, room.name.clone())));
        }

        Ok(all_events)
    }

    /// 特定のカレンダーから未来のイベント（進行中および今後予定されているもの）を取得
    async fn fetch_events_from_calendar(
        &self,
        calendar_id: &str,
    ) -> Result<Vec<Event>, RepositoryError> {
        // 過去24時間分も取得して、終了時刻でフィルタリングする
        // time_minを開始時刻で制限すると、現在進行中のイベント（開始時刻が過去）が除外されてしまう
        let time_min = Utc::now() - Duration::hours(24);

        let result = self
            .hub
            .events()
            .list(calendar_id)
            .time_min(time_min)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("Calendar API error: {}", e)))?;

        let now = Utc::now();
        let events = result.1.items.unwrap_or_default();

        // 終了時刻が現在時刻より後のイベントのみを返す
        // これにより、進行中または未来のイベントのみが対象となり、
        // 完了したイベントが誤って削除通知されるのを防ぐ
        let filtered_events: Vec<Event> = events
            .into_iter()
            .filter(|event| {
                event
                    .end
                    .as_ref()
                    .and_then(|e| e.date_time.as_ref())
                    .map(|end_time| *end_time > now)
                    .unwrap_or(false)
            })
            .collect();

        Ok(filtered_events)
    }

    /// イベントをResourceUsageに変換
    fn parse_event(
        &self,
        event: Event,
        resource_context: &str,
    ) -> Result<ResourceUsage, RepositoryError> {
        let id = UsageId::new(event.id.clone().unwrap_or_default());

        // owner_emailの決定ロジック
        let owner_email = event
            .creator
            .as_ref()
            .and_then(|c| c.email.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("作成者情報がありません".to_string()))?;

        // creatorがサービスアカウントの場合はattendeesから実際のユーザーを取得
        let owner_email = if owner_email == &self.service_account_email {
            event
                .attendees
                .as_ref()
                .and_then(|attendees| attendees.first())
                .and_then(|attendee| attendee.email.as_ref())
                .unwrap_or(owner_email) // attendeesが不正な場合はサービスアカウントをフォールバック
        } else {
            owner_email
        };

        let user = self.parse_user(owner_email)?;

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

        let time_period = TimePeriod::new(*start, *end)
            .map_err(|e| RepositoryError::Unknown(format!("時間枠エラー: {}", e)))?;

        // タイトルから資源をパース
        let default_title = String::new();
        let title = event.summary.as_ref().unwrap_or(&default_title);
        let items = self.parse_resources(title, resource_context)?;

        let notes = event.description.clone();

        ResourceUsage::new(id, user, time_period, items, notes).map_err(RepositoryError::from)
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
    /// 例: \[GPU(0), GPU(1), GPU(5)\] -> "0-1,5"
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

        // 連続する番号を範囲表記に変換
        let mut result = Vec::new();
        let mut i = 0;
        while i < device_numbers.len() {
            let start = device_numbers[i];
            let mut end = start;

            // 連続する番号を探す
            while i + 1 < device_numbers.len() && device_numbers[i + 1] == end + 1 {
                i += 1;
                end = device_numbers[i];
            }

            // 範囲表記または単一番号
            if start == end {
                result.push(start.to_string());
            } else if start + 1 == end {
                // 2つだけの場合は範囲表記を使わない
                result.push(start.to_string());
                result.push(end.to_string());
            } else {
                result.push(format!("{}-{}", start, end));
            }

            i += 1;
        }

        Some(result.join(","))
    }

    /// ResourceUsageから適切なカレンダーIDを取得
    fn get_calendar_id_for_usage(&self, usage: &ResourceUsage) -> Result<String, RepositoryError> {
        let resources = usage.resources();
        if resources.is_empty() {
            return Err(RepositoryError::Unknown("リソースが空です".to_string()));
        }

        match &resources[0] {
            Resource::Gpu(gpu) => {
                let server = self.config.get_server(gpu.server()).ok_or_else(|| {
                    RepositoryError::Unknown(format!("サーバーが見つかりません: {}", gpu.server()))
                })?;
                Ok(server.calendar_id.clone())
            }
            Resource::Room { name } => {
                let room = self
                    .config
                    .rooms
                    .iter()
                    .find(|r| &r.name == name)
                    .ok_or_else(|| {
                        RepositoryError::Unknown(format!("部屋が見つかりません: {}", name))
                    })?;
                Ok(room.calendar_id.clone())
            }
        }
    }

    /// ResourceUsageをGoogle Calendar Eventに変換
    fn create_event_from_usage(&self, usage: &ResourceUsage) -> Result<Event, RepositoryError> {
        let summary = match &usage.resources()[0] {
            Resource::Gpu(_) => self.format_gpu_spec(usage.resources()).ok_or_else(|| {
                RepositoryError::Unknown("GPUデバイス仕様の生成に失敗しました".to_string())
            })?,
            Resource::Room { name } => name.clone(),
        };

        let mut event = Event {
            summary: Some(summary),
            description: usage.notes().cloned(),
            start: Some(google_calendar3::api::EventDateTime {
                date_time: Some(usage.time_period().start()),
                ..Default::default()
            }),
            end: Some(google_calendar3::api::EventDateTime {
                date_time: Some(usage.time_period().end()),
                ..Default::default()
            }),
            // ゲスト（attendees）として実際のユーザーを登録
            attendees: Some(vec![google_calendar3::api::EventAttendee {
                email: Some(usage.owner_email().as_str().to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };

        // 既存のIDがある場合は設定（更新時）
        if !usage.id().as_str().is_empty() {
            event.id = Some(usage.id().as_str().to_string());
        }

        Ok(event)
    }

    /// 特定のカレンダーから特定のIDのイベントを取得
    async fn fetch_event_from_calendar(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        match self.hub.events().get(calendar_id, event_id).doit().await {
            Ok((_response, event)) => Ok(Some(event)),
            Err(e) => {
                // 404エラーの場合はNoneを返す
                if e.to_string().contains("404") {
                    Ok(None)
                } else {
                    Err(RepositoryError::ConnectionError(format!(
                        "Calendar API error: {}",
                        e
                    )))
                }
            }
        }
    }

    /// すべてのカレンダーから特定のIDのイベントを検索
    async fn find_event_across_calendars(
        &self,
        event_id: &str,
    ) -> Result<Option<(Event, String)>, RepositoryError> {
        // サーバーカレンダーから検索
        for server in &self.config.servers {
            if let Some(event) = self
                .fetch_event_from_calendar(&server.calendar_id, event_id)
                .await?
            {
                return Ok(Some((event, server.name.clone())));
            }
        }

        // 部屋カレンダーから検索
        for room in &self.config.rooms {
            if let Some(event) = self
                .fetch_event_from_calendar(&room.calendar_id, event_id)
                .await?
            {
                return Ok(Some((event, room.name.clone())));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl ResourceUsageRepository for GoogleCalendarUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        let event_id = id.as_str();

        if let Some((event, context)) = self.find_event_across_calendars(event_id).await? {
            let usage = self.parse_event(event, &context)?;
            Ok(Some(usage))
        } else {
            Ok(None)
        }
    }

    async fn find_all(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        // find_futureと同じ実装だが、時間フィルタリングなし
        // 現状ではfind_futureと同じにする（過去のイベントは通常管理対象外）
        self.find_future().await
    }

    async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let events = self.fetch_future_events().await?;

        let mut usages = Vec::new();
        for (event, context) in events {
            match self.parse_event(event, &context) {
                Ok(usage) => usages.push(usage),
                Err(e) => {
                    eprintln!("⚠️  イベントパースエラー: {}", e); // TODO@KinjiKawaguchi: エラーハンドリングの改善
                }
            }
        }

        Ok(usages)
    }

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let all_usages = self.find_future().await?;
        Ok(all_usages
            .into_iter()
            .filter(|usage| usage.time_period().overlaps_with(time_period))
            .collect())
    }

    async fn find_by_owner(
        &self,
        owner_email: &EmailAddress,
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let all_usages = self.find_future().await?;
        Ok(all_usages
            .into_iter()
            .filter(|usage| usage.owner_email() == owner_email)
            .collect())
    }

    async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError> {
        let calendar_id = self.get_calendar_id_for_usage(usage)?;
        let event = self.create_event_from_usage(usage)?;
        let event_id = usage.id().as_str();

        // IDが空の場合は新規作成、存在する場合は更新
        if event_id.is_empty() {
            // 新規作成
            self.hub
                .events()
                .insert(event, &calendar_id)
                .doit()
                .await
                .map_err(|e| {
                    RepositoryError::ConnectionError(format!("イベント作成に失敗: {}", e))
                })?;
        } else {
            // 既存のイベントが存在するか確認
            if self.find_event_across_calendars(event_id).await?.is_some() {
                // 更新
                self.hub
                    .events()
                    .update(event, &calendar_id, event_id)
                    .doit()
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!("イベント更新に失敗: {}", e))
                    })?;
            } else {
                // IDが指定されているが存在しない場合は新規作成
                self.hub
                    .events()
                    .insert(event, &calendar_id)
                    .doit()
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!("イベント作成に失敗: {}", e))
                    })?;
            }
        }

        Ok(())
    }

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError> {
        let event_id = id.as_str();

        // すべてのカレンダーから検索
        if let Some((_, context)) = self.find_event_across_calendars(event_id).await? {
            // カレンダーIDを取得
            let calendar_id =
                if let Some(server) = self.config.servers.iter().find(|s| s.name == context) {
                    &server.calendar_id
                } else if let Some(room) = self.config.rooms.iter().find(|r| r.name == context) {
                    &room.calendar_id
                } else {
                    return Err(RepositoryError::Unknown(format!(
                        "カレンダーが見つかりません: {}",
                        context
                    )));
                };

            // イベントを削除
            self.hub
                .events()
                .delete(calendar_id, event_id)
                .doit()
                .await
                .map_err(|e| {
                    RepositoryError::ConnectionError(format!("イベント削除に失敗: {}", e))
                })?;

            Ok(())
        } else {
            Err(RepositoryError::NotFound)
        }
    }
}
