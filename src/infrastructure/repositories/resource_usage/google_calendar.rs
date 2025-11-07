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

pub struct GoogleCalendarUsageRepository {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
    config: ResourceConfig,
}

impl GoogleCalendarUsageRepository {
    pub async fn new(
        service_account_key: &str,
        config: ResourceConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let secret = yup_oauth2::read_service_account_key(service_account_key).await?;

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

        Ok(Self { hub, config })
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

        let creator_email = event
            .creator
            .as_ref()
            .and_then(|c| c.email.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("作成者情報がありません".to_string()))?;

        let user = self.parse_user(creator_email)?;

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
}

#[async_trait]
impl ResourceUsageRepository for GoogleCalendarUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        // Google Calendar APIの制約上、未来のイベントのみから検索
        let future = self.find_future().await?;
        Ok(future.into_iter().find(|u| u.id().as_str() == id.as_str()))
    }

    async fn find_all(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        Err(RepositoryError::Unknown(
            "find_all機能は未実装です。Google Calendar APIの制約上、過去のイベントを全て取得することは現実的ではありません。find_future()を使用してください".to_string()
        ))
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
        // Google Calendar APIの制約上、未来のイベントのみから検索
        let future = self.find_future().await?;
        Ok(future
            .into_iter()
            .filter(|u| u.time_period().overlaps_with(time_period))
            .collect())
    }

    async fn save(&self, _usage: &ResourceUsage) -> Result<(), RepositoryError> {
        Err(RepositoryError::Unknown("save機能は未実装です".to_string()))
    }

    async fn delete(&self, _id: &UsageId) -> Result<(), RepositoryError> {
        Err(RepositoryError::Unknown(
            "delete機能は未実装です".to_string(),
        ))
    }
}
