use super::event_gateway::{CalendarEventGateway, GoogleCalendarEventGateway};
use super::id_mapper::{ExternalId, IdMapper};
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::{
    entity::ResourceUsage,
    factory::ResourceFactory,
    value_objects::{Resource, TimePeriod, UsageId},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, RepositoryError, ResourceUsageRepository,
};
use crate::infrastructure::config::ResourceConfig;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use google_calendar3::{
    CalendarHub,
    api::Event,
    hyper_rustls::HttpsConnectorBuilder,
    hyper_util::{client::legacy::Client, rt::TokioExecutor},
    yup_oauth2,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// アプリ経由で作成されたイベントのdescriptionに付与される、予約者メールアドレス行の接頭辞
const OWNER_LINE_PREFIX: &str = "予約者: ";

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

/// キャンセル済みイベントのstatus値
const EVENT_STATUS_CANCELLED: &str = "cancelled";

/// イベントが有効な（キャンセルされていない）予約かどうか
///
/// 繰り返しイベントを個々の予定に展開して取得すると、削除された回が
/// `status: cancelled`として返ることがある。これは予約として扱わない。
fn is_active_event(event: &Event) -> bool {
    event.status.as_deref() != Some(EVENT_STATUS_CANCELLED)
}

/// Google Calendar APIを使用したResourceUsageリポジトリ実装
pub struct GoogleCalendarUsageRepository {
    gateway: Arc<dyn CalendarEventGateway>,
    config: ResourceConfig,
    service_account_email: String,
    id_mapper: Arc<IdMapper>,
    /// 予約者の付加情報（OSユーザー名など）を引くためのリポジトリ
    identity_repo: Arc<dyn IdentityLinkRepository>,
    /// 解釈失敗を既に警告したイベントID
    ///
    /// 同じイベントは取得のたびに失敗するため、警告を出し続けるとログが埋まる。
    reported_parse_failures: Mutex<HashSet<String>>,
}

impl GoogleCalendarUsageRepository {
    /// 新しいGoogle Calendarリポジトリを作成
    ///
    /// # Arguments
    /// * `service_account_key` - サービスアカウントキーファイルのパス
    /// * `config` - リソース設定
    /// * `id_mappings_path` - IDマッピングファイルのパス
    pub async fn new(
        service_account_key: &str,
        config: ResourceConfig,
        id_mappings_path: std::path::PathBuf,
        identity_repo: Arc<dyn IdentityLinkRepository>,
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

        let id_mapper = IdMapper::new(id_mappings_path)?;

        Ok(Self {
            gateway: Arc::new(GoogleCalendarEventGateway::new(hub)),
            config,
            service_account_email,
            id_mapper: Arc::new(id_mapper),
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
        id_mappings_path: std::path::PathBuf,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Result<Self, RepositoryError> {
        Ok(Self {
            gateway,
            config,
            service_account_email,
            id_mapper: Arc::new(IdMapper::new(id_mappings_path)?),
            identity_repo,
            reported_parse_failures: Mutex::new(HashSet::new()),
        })
    }

    /// イベントのdescriptionを組み立てる
    ///
    /// アプリが管理する行はマーカーで囲む。カレンダーを開いた人が状況を把握できるよう、
    /// 予約者のメールアドレスに加えて、分かる範囲の情報（OSユーザー名・予約ID）を並べる。
    /// 備考はマーカーの外に置き、利用者が自由に編集できる領域として残す。
    pub(super) async fn build_description(&self, usage: &ResourceUsage) -> String {
        let mut lines = vec![
            MANAGED_SECTION_BEGIN.to_string(),
            format!("{OWNER_LINE_PREFIX}{}", usage.owner_email().as_str()),
        ];

        if let Some(os_user) = self.resolve_os_user_name(usage).await {
            lines.push(format!("{OS_USER_LINE_PREFIX}{os_user}"));
        }

        lines.push(format!(
            "{RESERVATION_ID_LINE_PREFIX}{}",
            usage.id().as_str()
        ));
        lines.push(MANAGED_SECTION_END.to_string());

        let mut description = lines.join("\n");

        if let Some(notes) = usage.notes() {
            description.push_str(&format!("\n\n{notes}"));
        }

        description
    }

    /// 予約対象サーバーにおける予約者のOSユーザー名を引く
    ///
    /// OSユーザー名の名前空間はサーバーごとに異なりうるため、予約したサーバーの紐付けを見る。
    /// 部屋の予約や紐付けが無い場合はNoneを返す。表示のための情報であり、
    /// 引けないことを理由に予約の保存を失敗させない。
    async fn resolve_os_user_name(&self, usage: &ResourceUsage) -> Option<String> {
        let server = usage
            .resources()
            .iter()
            .find_map(|resource| match resource {
                Resource::Gpu(gpu) => Some(gpu.server().to_string()),
                Resource::Room { .. } => None,
            })?;

        let identity_link = self
            .identity_repo
            .find_by_email(usage.owner_email())
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    "予約者の紐付け取得に失敗しました（OS名は省略します）: {}",
                    e
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
        self.config
            .servers
            .iter()
            .map(|server| (server.calendar_id.clone(), server.name.clone()))
            .chain(
                self.config
                    .rooms
                    .iter()
                    .map(|room| (room.calendar_id.clone(), room.name.clone())),
            )
            .collect()
    }

    /// すべてのカレンダーから、指定期間に重なるイベントを取得
    ///
    /// `time_min`は「イベントの終了時刻がこれより後」、`time_max`は「イベントの開始時刻が
    /// これより前」を意味する。どの期間を対象とするかは呼び出し側が決める。
    ///
    /// 戻り値: (Event, calendar_id, resource_name)
    async fn fetch_events(
        &self,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<(Event, String, String)>, RepositoryError> {
        let mut all_events = Vec::new();

        for (calendar_id, resource_name) in self.calendars() {
            let events = self
                .gateway
                .list_events(&calendar_id, time_min, time_max)
                .await?;

            all_events.extend(
                events
                    .into_iter()
                    .filter(is_active_event)
                    .map(|event| (event, calendar_id.clone(), resource_name.clone())),
            );
        }

        Ok(all_events)
    }

    /// 解釈失敗を警告として報告すべきか（イベントごとに初回のみ）
    pub(super) fn should_report_parse_failure(&self, event_id: &str) -> bool {
        self.reported_parse_failures
            .lock()
            .expect("解釈失敗の記録ロックを取得できませんでした")
            .insert(event_id.to_string())
    }

    /// 取得したイベント群をResourceUsageへ変換する
    ///
    /// 予約として解釈できないイベント（カレンダー上で直接作られた自由記述のイベントなど）は
    /// 警告を残して飛ばす。1件の解釈失敗で予約機能全体を止めないための判断。
    fn parse_events(&self, events: Vec<(Event, String, String)>) -> Vec<ResourceUsage> {
        events
            .into_iter()
            .filter_map(|(event, calendar_id, resource_context)| {
                let event_id = event.id.clone().unwrap_or_default();
                match self.parse_event(event, &calendar_id, &resource_context) {
                    Ok(usage) => Some(usage),
                    Err(e) => {
                        if self.should_report_parse_failure(&event_id) {
                            tracing::warn!(
                                "イベントを予約として解釈できませんでした: calendar_id={}, event_id={}, error={}",
                                calendar_id,
                                event_id,
                                e
                            );
                        } else {
                            tracing::debug!(
                                "イベントを予約として解釈できませんでした(既報): event_id={}",
                                event_id
                            );
                        }
                        None
                    }
                }
            })
            .collect()
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

        let domain_id = match self.id_mapper.get_domain_id(&event_id)? {
            Some(existing_domain_id) => existing_domain_id,
            None => {
                // マッピングが見つからない場合、新しいdomain_idを生成してマッピングを作成
                let new_domain_id = UsageId::new();

                // 新しいマッピングを保存
                self.id_mapper.save_mapping(
                    new_domain_id.as_str(),
                    ExternalId {
                        calendar_id: calendar_id.to_string(),
                        event_id: event_id.clone(),
                    },
                )?;

                new_domain_id.as_str().to_string()
            }
        };

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
                    // "予約者: user@example.com" 行をアプリ管理セクション内から探して抽出
                    desc.lines()
                        .find_map(|line| line.strip_prefix(OWNER_LINE_PREFIX))
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

        let notes = event
            .description
            .as_ref()
            .and_then(|desc| Self::extract_notes(desc));

        ResourceUsage::reconstruct(id, user, time_period, items, notes)
            .map_err(RepositoryError::from)
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
        } else if description.starts_with(OWNER_LINE_PREFIX) {
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
    async fn create_event_from_usage(
        &self,
        usage: &ResourceUsage,
    ) -> Result<Event, RepositoryError> {
        // 注: get_calendar_id_for_usageで検証済みのため、resources()[0]は安全に使用できる
        let summary = match &usage.resources()[0] {
            Resource::Gpu(_) => self.format_gpu_spec(usage.resources()).ok_or_else(|| {
                RepositoryError::Unknown("GPUデバイス仕様の生成に失敗しました".to_string())
            })?,
            Resource::Room { name } => name.clone(),
        };

        let description = self.build_description(usage).await;

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
        // 各カレンダーでイベントの検索を試みる
        for (calendar_id, resource_context) in self.calendars() {
            match self.gateway.get_event(&calendar_id, event_id).await? {
                Some(event) => {
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
        // 各カレンダーでイベントの削除を試みる
        for (calendar_id, _resource_name) in self.calendars() {
            match self.gateway.delete_event(&calendar_id, event_id).await {
                Ok(_) => {
                    tracing::info!(
                        "✅ イベント削除成功: event_id={}, calendar_id={}",
                        event_id,
                        calendar_id
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!(
                        "カレンダー {} でイベント {} が見つかりませんでした: {}",
                        calendar_id,
                        event_id,
                        e
                    );
                    // 次のカレンダーを試す
                    continue;
                }
            }
        }

        // すべてのカレンダーで見つからなかった
        tracing::error!(
            "❌ イベントが全カレンダーで見つかりませんでした: event_id={}",
            event_id
        );
        Err(RepositoryError::NotFound)
    }
}

#[async_trait]
impl ResourceUsageRepository for GoogleCalendarUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        let input_id = id.as_str();

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
            .gateway
            .get_event(&external_id.calendar_id, &external_id.event_id)
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
        // 終了時刻が現在より後のイベントに限る。開始時刻では絞らないため、
        // 進行中のイベント（開始時刻が過去）も含まれる。
        let events = self.fetch_events(Utc::now(), None).await?;

        Ok(self.parse_events(events))
    }

    /// 指定期間と重複するResourceUsageを検索
    ///
    /// 対象期間そのものをカレンダーに問い合わせる。`find_future`は使わない。
    /// 事後予約のように過去の期間を予約する場合、相手の予約が既に終了していても
    /// 競合は競合であり、終了済みを除外すると重複予約を許してしまう。
    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let events = self
            .fetch_events(time_period.start(), Some(time_period.end()))
            .await?;

        Ok(self
            .parse_events(events)
            .into_iter()
            .filter(|usage| usage.time_period().overlaps_with(time_period))
            .collect())
    }

    /// 特定のユーザーが所有するResourceUsageを検索
    ///
    /// 進行中および今後の予約のみを対象とする（終了済みの予約は返さない）。
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

        // Domain IDから外部IDを検索
        if let Some(external_id) = self.id_mapper.get_external_id(domain_id)? {
            // 既存イベント
            if external_id.calendar_id == new_calendar_id {
                // 同じカレンダー → 更新
                // IMPORTANT: update API用に id フィールドを含む Event を作成
                let mut event = self.create_event_from_usage(usage).await?;
                event.id = Some(external_id.event_id.clone());

                self.gateway
                    .update_event(&external_id.calendar_id, &external_id.event_id, event)
                    .await?;
            } else {
                // カレンダーが変更された → 古いカレンダーから削除し、新しいカレンダーに作成
                // 古いイベントを削除
                self.gateway
                    .delete_event(&external_id.calendar_id, &external_id.event_id)
                    .await
                    .map_err(|e| {
                        RepositoryError::ConnectionError(format!("古いイベントの削除に失敗: {}", e))
                    })?;

                // 新しいカレンダーにイベントを作成
                let event = self.create_event_from_usage(usage).await?;
                let created_event = self
                    .gateway
                    .insert_event(&new_calendar_id, event)
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
            let event = self.create_event_from_usage(usage).await?;
            let created_event = self.gateway.insert_event(&new_calendar_id, event).await?;

            // Event IDを取得してマッピングを保存
            let event_id = created_event.id.ok_or_else(|| {
                RepositoryError::Unknown("作成されたイベントにIDがありません".to_string())
            })?;

            self.id_mapper.save_mapping(
                domain_id,
                ExternalId {
                    calendar_id: new_calendar_id.clone(),
                    event_id: event_id.clone(),
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
        self.gateway
            .delete_event(&external_id.calendar_id, &external_id.event_id)
            .await?;

        // マッピングを削除
        self.id_mapper.delete_mapping(&actual_domain_id)?;

        Ok(())
    }
}
