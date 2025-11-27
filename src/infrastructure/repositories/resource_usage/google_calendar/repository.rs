use super::id_mapper::{ExternalId, IdMapper};
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
use std::sync::Arc;

/// Google Calendar APIを使用したResourceUsageリポジトリ実装
pub struct GoogleCalendarUsageRepository {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
    config: ResourceConfig,
    service_account_email: String,
    id_mapper: Arc<IdMapper>,
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

        // ID マッピングの初期化
        // TODO(#41): マッピングファイルパスを設定ファイルまたは環境変数から読み込む
        let id_mapper = IdMapper::new(std::path::PathBuf::from(
            "data/google_calendar_mappings.json",
        ))?;

        Ok(Self {
            hub,
            config,
            service_account_email,
            id_mapper: Arc::new(id_mapper),
        })
    }

    /// すべてのカレンダーから未来のイベントを取得
    /// 戻り値: (Event, calendar_id, resource_name)
    async fn fetch_future_events(&self) -> Result<Vec<(Event, String, String)>, RepositoryError> {
        let mut all_events = Vec::new();

        // 各サーバーカレンダーから取得
        for server in &self.config.servers {
            let events = self.fetch_events_from_calendar(&server.calendar_id).await?;
            all_events.extend(
                events
                    .into_iter()
                    .map(|e| (e, server.calendar_id.clone(), server.name.clone())),
            );
        }

        // 部屋カレンダーから取得
        for room in &self.config.rooms {
            let events = self.fetch_events_from_calendar(&room.calendar_id).await?;
            all_events.extend(
                events
                    .into_iter()
                    .map(|e| (e, room.calendar_id.clone(), room.name.clone())),
            );
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
        calendar_id: &str,
        resource_context: &str,
    ) -> Result<ResourceUsage, RepositoryError> {
        // Event ID から Domain ID を取得
        let event_id = event.id.clone().unwrap_or_default();

        tracing::info!("📝 parse_event: event_id={}", event_id);

        let domain_id = match self.id_mapper.get_domain_id(&event_id)? {
            Some(existing_domain_id) => {
                tracing::info!("  → 既存マッピング発見: domain_id={}", existing_domain_id);
                existing_domain_id
            }
            None => {
                // マッピングが見つからない場合、新しいdomain_idを生成してマッピングを作成
                let new_domain_id = UsageId::new();

                tracing::info!("  → マッピングなし。新しいdomain_idを生成: {}", new_domain_id.as_str());

                // 新しいマッピングを保存
                self.id_mapper.save_mapping(
                    new_domain_id.as_str(),
                    ExternalId {
                        calendar_id: calendar_id.to_string(),
                        event_id: event_id.clone(),
                    },
                )?;

                tracing::info!("  → マッピング保存完了");

                new_domain_id.as_str().to_string()
            }
        };

        tracing::info!("  → 使用するdomain_id={}", domain_id);

        let id = UsageId::from_string(domain_id);

        // owner_emailの決定ロジック
        let owner_email = event
            .creator
            .as_ref()
            .and_then(|c| c.email.as_ref())
            .ok_or_else(|| RepositoryError::Unknown("作成者情報がありません".to_string()))?;

        // creatorがサービスアカウントの場合はdescriptionから実際のユーザーを取得
        let owner_email = if owner_email == &self.service_account_email {
            event
                .description
                .as_ref()
                .and_then(|desc| {
                    // "予約者: user@example.com" の形式から抽出
                    desc.lines()
                        .next()
                        .and_then(|line| line.strip_prefix("予約者: "))
                })
                .ok_or_else(|| {
                    RepositoryError::Unknown(
                        "サービスアカウントで作成されたイベントのdescriptionにユーザー情報がありません"
                            .to_string(),
                    )
                })?
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

        // descriptionから備考を抽出（"予約者: xxx"の行を除外）
        let notes = event.description.as_ref().and_then(|desc| {
            // "予約者: xxx\n\n備考" の形式から備考部分を抽出
            desc.split_once("\n\n").map(|(_, notes)| notes.to_string())
        });

        ResourceUsage::reconstruct(id, user, time_period, items, notes)
            .map_err(RepositoryError::from)
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

    /// ResourceUsageから適切なカレンダーIDを取得
    ///
    /// # 前提条件
    /// このメソッドは、ResourceUsage内のすべてのリソースが同一のカレンダーに属することを前提としています。
    /// （例: すべてGPU、またはすべて部屋）
    /// 混在している場合はエラーを返します。
    fn get_calendar_id_for_usage(&self, usage: &ResourceUsage) -> Result<String, RepositoryError> {
        let resources = usage.resources();
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
    ///
    /// # 前提条件
    /// このメソッドは、get_calendar_id_for_usageで検証済みのResourceUsageを受け取ることを前提としています。
    /// すなわち、すべてのリソースが同一のカレンダーに属していることが保証されています。
    fn create_event_from_usage(&self, usage: &ResourceUsage) -> Result<Event, RepositoryError> {
        // 注: get_calendar_id_for_usageで検証済みのため、resources()[0]は安全に使用できる
        let summary = match &usage.resources()[0] {
            Resource::Gpu(_) => self.format_gpu_spec(usage.resources()).ok_or_else(|| {
                RepositoryError::Unknown("GPUデバイス仕様の生成に失敗しました".to_string())
            })?,
            Resource::Room { name } => name.clone(),
        };

        // descriptionに予約者情報と備考を含める
        let description = {
            let mut desc = format!("予約者: {}", usage.owner_email().as_str());
            if let Some(notes) = usage.notes() {
                desc.push_str(&format!("\n\n{}", notes));
            }
            desc
        };

        Ok(Event {
            summary: Some(summary),
            description: Some(description),
            start: Some(google_calendar3::api::EventDateTime {
                date_time: Some(usage.time_period().start()),
                ..Default::default()
            }),
            end: Some(google_calendar3::api::EventDateTime {
                date_time: Some(usage.time_period().end()),
                ..Default::default()
            }),
            // NOTE: attendeesを追加するとDomain-Wide Delegationが必要になるため、
            // 予約者情報はdescriptionに含めています
            // NOTE: Event IDはGoogle Calendar側で自動生成され、id_mapperで管理されます
            ..Default::default()
        })
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
                // HTTPステータスコード404の場合はNoneを返す
                // google_calendar3のエラーは構造化されていないため、
                // エラーメッセージから404を検出する
                // TODO(#41): 文字列マッチングは脆弱。構造化されたエラー型またはHTTPステータスコードを直接チェック
                let error_msg = e.to_string();
                if error_msg.contains("404") || error_msg.contains("Not Found") {
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

    /// カレンダーIDからリソースコンテキスト（サーバー名または部屋名）を取得
    fn get_resource_context(&self, calendar_id: &str) -> Result<String, RepositoryError> {
        // サーバーカレンダーから検索
        for server in &self.config.servers {
            if server.calendar_id == calendar_id {
                return Ok(server.name.clone());
            }
        }

        // 部屋カレンダーから検索
        for room in &self.config.rooms {
            if room.calendar_id == calendar_id {
                return Ok(room.name.clone());
            }
        }

        Err(RepositoryError::Unknown(format!(
            "カレンダーIDに対応するリソースが見つかりません: {}",
            calendar_id
        )))
    }

    /// event_idから直接イベントを検索（マッピングがない場合）
    ///
    /// 全カレンダーから該当するイベントを検索してResourceUsageを返します。
    async fn find_by_event_id(
        &self,
        event_id: &str,
    ) -> Result<Option<ResourceUsage>, RepositoryError> {
        // すべてのカレンダーIDを取得
        let mut calendar_ids: Vec<String> = self
            .config
            .servers
            .iter()
            .map(|server| server.calendar_id.clone())
            .collect();

        // 部屋のカレンダーも追加
        for room in &self.config.rooms {
            calendar_ids.push(room.calendar_id.clone());
        }

        // 各カレンダーでイベントの検索を試みる
        for calendar_id in calendar_ids {
            match self
                .fetch_event_from_calendar(&calendar_id, event_id)
                .await?
            {
                Some(event) => {
                    // リソースコンテキストを取得
                    let resource_context = self.get_resource_context(&calendar_id)?;
                    // イベントをパース（この時点で新しいマッピングが作成される）
                    let usage = self.parse_event(event, &calendar_id, &resource_context)?;
                    return Ok(Some(usage));
                }
                None => {
                    // 次のカレンダーを試す
                    continue;
                }
            }
        }

        // すべてのカレンダーで見つからなかった
        Ok(None)
    }

    /// event_idから直接イベントを削除（マッピングがない場合）
    ///
    /// 全カレンダーから該当するイベントを検索して削除します。
    async fn delete_by_event_id(&self, event_id: &str) -> Result<(), RepositoryError> {
        // すべてのカレンダーIDを取得
        let mut calendar_ids: Vec<String> = self
            .config
            .servers
            .iter()
            .map(|server| server.calendar_id.clone())
            .collect();

        // 部屋のカレンダーも追加
        for room in &self.config.rooms {
            calendar_ids.push(room.calendar_id.clone());
        }

        // 各カレンダーでイベントの削除を試みる
        for calendar_id in calendar_ids {
            match self
                .hub
                .events()
                .delete(&calendar_id, event_id)
                .doit()
                .await
            {
                Ok(_) => {
                    return Ok(());
                }
                Err(_) => {
                    // 次のカレンダーを試す
                    continue;
                }
            }
        }

        // すべてのカレンダーで見つからなかった
        Err(RepositoryError::NotFound)
    }
}

#[async_trait]
impl ResourceUsageRepository for GoogleCalendarUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        let input_id = id.as_str();

        tracing::info!("🔍 find_by_id: input_id={}", input_id);

        // まずdomain_idとして外部IDを取得を試みる
        let external_id = match self.id_mapper.get_external_id(input_id)? {
            Some(ext_id) => ext_id,
            None => {
                // 見つからない場合、input_idがevent_idの可能性がある
                // 逆引きマッピングを試みる
                match self.id_mapper.get_domain_id(input_id)? {
                    Some(domain_id) => {
                        // domain_idが見つかったので、それで外部IDを取得
                        match self.id_mapper.get_external_id(&domain_id)? {
                            Some(ext_id) => ext_id,
                            None => {
                                return Ok(None);
                            }
                        }
                    }
                    None => {
                        // それでも見つからない場合、event_idとして全カレンダーから検索
                        return self.find_by_event_id(input_id).await;
                    }
                }
            }
        };

        // 特定のカレンダーから直接イベントを取得
        let event = match self
            .fetch_event_from_calendar(&external_id.calendar_id, &external_id.event_id)
            .await?
        {
            Some(event) => event,
            None => return Ok(None), // イベントが見つからない場合はNone
        };

        // リソースコンテキストを取得
        let resource_context = self.get_resource_context(&external_id.calendar_id)?;

        // イベントをパース（ただし、domain_idは元のinput_idを使用）
        let mut usage = self.parse_event(event, &external_id.calendar_id, &resource_context)?;

        // IMPORTANT: find_by_id() で検索した場合、取得したResourceUsageのIDは
        // 必ず元のinput_idであるべき。parse_event()が別のdomain_idを生成した場合、
        // それを元のinput_idで上書きする。
        if usage.id().as_str() != input_id {
            tracing::warn!(
                "parse_event returned different domain_id: expected={}, got={}. Overriding with expected ID.",
                input_id,
                usage.id().as_str()
            );
            // ResourceUsageのIDを元のinput_idに置き換える
            usage = ResourceUsage::reconstruct(
                UsageId::from_string(input_id.to_string()),
                usage.owner_email().clone(),
                usage.time_period().clone(),
                usage.resources().to_vec(),
                usage.notes().cloned(),
            )?;
        }

        Ok(Some(usage))
    }

    async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let events = self.fetch_future_events().await?;

        let mut usages = Vec::new();
        for (event, calendar_id, context) in events {
            match self.parse_event(event, &calendar_id, &context) {
                Ok(usage) => usages.push(usage),
                Err(e) => {
                    eprintln!("⚠️  イベントパースエラー: {}", e); // TODO@KinjiKawaguchi: エラーハンドリングの改善
                }
            }
        }

        Ok(usages)
    }

    /// 指定期間と重複するResourceUsageを検索
    ///
    /// # パフォーマンスに関する注意
    /// 現在の実装では、すべての未来のイベントを取得してからメモリ上でフィルタリングしています。
    /// Google Calendar APIには時間範囲での検索機能がありますが、複数カレンダーにまたがる
    /// 検索を効率的に行うための十分なクエリ機能がないため、この実装を採用しています。
    ///
    /// 将来的な改善案:
    /// - 各カレンダーに対して時間範囲クエリを並列実行
    /// - 結果のキャッシング（短時間の重複チェックに有効）
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

    /// 特定のユーザーが所有するResourceUsageを検索
    ///
    /// # パフォーマンスに関する注意
    /// 現在の実装では、すべての未来のイベントを取得してからメモリ上でフィルタリングしています。
    /// Google Calendar APIには所有者による検索機能がありますが、複数カレンダーにまたがる
    /// 検索と、descriptionフィールドからの所有者抽出が必要なため、この実装を採用しています。
    ///
    /// 将来的な改善案:
    /// - ユーザーごとのイベントキャッシング
    /// - 定期的なバックグラウンド同期によるローカルインデックス構築
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
        let new_calendar_id = self.get_calendar_id_for_usage(usage)?;
        let domain_id = usage.id().as_str();

        tracing::info!("💾 save: domain_id={}", domain_id);

        // Domain IDから外部IDを検索
        if let Some(external_id) = self.id_mapper.get_external_id(domain_id)? {
            tracing::info!("  → 既存イベントとして更新: calendar_id={}, event_id={}", external_id.calendar_id, external_id.event_id);
            // 既存イベント
            if external_id.calendar_id == new_calendar_id {
                // 同じカレンダー → 更新
                // IMPORTANT: update API用に id フィールドを含む Event を作成
                let mut event = self.create_event_from_usage(usage)?;
                event.id = Some(external_id.event_id.clone());

                self.hub
                    .events()
                    .update(event, &external_id.calendar_id, &external_id.event_id)
                    .doit()
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!("イベント更新に失敗: {}", e))
                    })?;
            } else {
                // カレンダーが変更された → 古いカレンダーから削除し、新しいカレンダーに作成
                // 古いイベントを削除
                self.hub
                    .events()
                    .delete(&external_id.calendar_id, &external_id.event_id)
                    .doit()
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!("古いイベントの削除に失敗: {}", e))
                    })?;

                // 新しいカレンダーにイベントを作成
                let event = self.create_event_from_usage(usage)?;
                let (_response, created_event) = self
                    .hub
                    .events()
                    .insert(event, &new_calendar_id)
                    .doit()
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!(
                            "新しいカレンダーへのイベント作成に失敗: {}",
                            e
                        ))
                    })?;

                // 新しいEvent IDを取得してマッピングを更新
                let new_event_id = created_event.id.ok_or_else(|| {
                    RepositoryError::Unknown("作成されたイベントにIDがありません".to_string())
                })?;

                self.id_mapper.save_mapping(
                    domain_id,
                    ExternalId {
                        calendar_id: new_calendar_id,
                        event_id: new_event_id,
                    },
                )?;
            }
        } else {
            // 新規 → 作成
            tracing::info!("  → 新規イベントとして作成");
            let event = self.create_event_from_usage(usage)?;
            let (_response, created_event) = self
                .hub
                .events()
                .insert(event, &new_calendar_id)
                .doit()
                .await
                .map_err(|e| {
                    RepositoryError::ConnectionError(format!("イベント作成に失敗: {}", e))
                })?;

            // Event IDを取得してマッピングを保存
            let event_id = created_event.id.ok_or_else(|| {
                RepositoryError::Unknown("作成されたイベントにIDがありません".to_string())
            })?;

            self.id_mapper.save_mapping(
                domain_id,
                ExternalId {
                    calendar_id: new_calendar_id,
                    event_id,
                },
            )?;
        }

        Ok(())
    }

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError> {
        let input_id = id.as_str();

        // まずdomain_idとして外部IDを取得を試みる
        let (external_id, actual_domain_id) = match self.id_mapper.get_external_id(input_id)? {
            Some(ext_id) => (ext_id, input_id.to_string()),
            None => {
                // 見つからない場合、input_idがevent_idの可能性がある
                // 逆引きマッピングを試みる
                match self.id_mapper.get_domain_id(input_id)? {
                    Some(domain_id) => {
                        // domain_idが見つかったので、それで外部IDを取得
                        let ext_id = self
                            .id_mapper
                            .get_external_id(&domain_id)?
                            .ok_or(RepositoryError::NotFound)?;
                        (ext_id, domain_id)
                    }
                    None => {
                        // それでも見つからない場合、input_idを直接event_idとして使用
                        // カレンダーIDを推定する必要がある
                        // とりあえず、全カレンダーから検索して削除を試みる
                        return self.delete_by_event_id(input_id).await;
                    }
                }
            }
        };

        // イベントを削除
        self.hub
            .events()
            .delete(&external_id.calendar_id, &external_id.event_id)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("イベント削除に失敗: {}", e)))?;

        // マッピングを削除
        self.id_mapper.delete_mapping(&actual_domain_id)?;

        Ok(())
    }
}
