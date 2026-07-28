//! LRM MCPサーバー
//!
//! 既存のユースケース層をそのまま呼び出す薄いアダプタ。新しい業務ロジックは持たない。

use crate::application::usecases::create_resource_usage::CreateResourceUsageUseCase;
use crate::application::usecases::delete_resource_usage::DeleteResourceUsageUseCase;
use crate::application::usecases::get_resource_usage_by_id::GetResourceUsageByIdUseCase;
use crate::application::usecases::list_all_future_resource_usages::ListAllFutureResourceUsagesUseCase;
use crate::application::usecases::list_user_resource_usages::ListUserResourceUsagesUseCase;
use crate::application::usecases::update_resource_usage::UpdateResourceUsageUseCase;
use crate::domain::aggregates::resource_usage::value_objects::{
    Gpu, Resource, TimePeriod, UsageId,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::infrastructure::config::ResourceConfig;
use crate::interface::mcp::auth::ResolvedCaller;
use crate::interface::mcp::dto::{
    CancelReservationParams, CreateReservationParams, GetReservationParams, ReservationDto,
    UpdateReservationParams,
};
use rmcp::ErrorData;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use std::sync::Arc;

/// LRMの予約をMCPツールとして公開するサーバー
///
/// 全フィールドが`Arc`のため安価に`Clone`でき、接続ごとに新しいインスタンスを渡せる。
/// `R: Clone`を要求しないよう`Clone`は手動実装する（`derive(Clone)`は`R: Clone`を要求してしまう）。
pub struct LrmMcpServer<R: ResourceUsageRepository> {
    create_usecase: Arc<CreateResourceUsageUseCase<R>>,
    update_usecase: Arc<UpdateResourceUsageUseCase<R>>,
    delete_usecase: Arc<DeleteResourceUsageUseCase<R>>,
    list_all_usecase: Arc<ListAllFutureResourceUsagesUseCase<R>>,
    list_mine_usecase: Arc<ListUserResourceUsagesUseCase<R>>,
    get_by_id_usecase: Arc<GetResourceUsageByIdUseCase<R>>,
    resource_config: Arc<ResourceConfig>,
}

impl<R: ResourceUsageRepository> Clone for LrmMcpServer<R> {
    fn clone(&self) -> Self {
        Self {
            create_usecase: self.create_usecase.clone(),
            update_usecase: self.update_usecase.clone(),
            delete_usecase: self.delete_usecase.clone(),
            list_all_usecase: self.list_all_usecase.clone(),
            list_mine_usecase: self.list_mine_usecase.clone(),
            get_by_id_usecase: self.get_by_id_usecase.clone(),
            resource_config: self.resource_config.clone(),
        }
    }
}

/// 認証ミドルウェアが挿入した呼び出し元メールアドレスを取り出す
///
/// ミドルウェアで既に未認証は401で弾いているため、ここに到達して取得できない場合は
/// ミドルウェアの配線漏れを意味するプロトコルレベルの内部エラーとして扱う。
fn resolved_caller(parts: &http::request::Parts) -> Result<EmailAddress, ErrorData> {
    parts
        .extensions
        .get::<ResolvedCaller>()
        .map(|caller| caller.0.clone())
        .ok_or_else(|| {
            ErrorData::internal_error(
                "認証情報が見つかりません（認証ミドルウェアの設定を確認してください）",
                None,
            )
        })
}

/// アプリケーションエラーをツールレベルのエラーとして呼び出し元に見せる
fn tool_error(message: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.to_string())])
}

/// 値をJSONにシリアライズしてツール成功結果として返す
fn success_json<T: serde::Serialize>(value: &T) -> CallToolResult {
    match serde_json::to_string_pretty(value) {
        Ok(json) => CallToolResult::success(vec![ContentBlock::text(json)]),
        Err(e) => tool_error(format!("シリアライズに失敗しました: {}", e)),
    }
}

#[tool_router]
impl<R> LrmMcpServer<R>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
{
    /// 新しいLrmMcpServerを作成
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        create_usecase: Arc<CreateResourceUsageUseCase<R>>,
        update_usecase: Arc<UpdateResourceUsageUseCase<R>>,
        delete_usecase: Arc<DeleteResourceUsageUseCase<R>>,
        list_all_usecase: Arc<ListAllFutureResourceUsagesUseCase<R>>,
        list_mine_usecase: Arc<ListUserResourceUsagesUseCase<R>>,
        get_by_id_usecase: Arc<GetResourceUsageByIdUseCase<R>>,
        resource_config: Arc<ResourceConfig>,
    ) -> Self {
        Self {
            create_usecase,
            update_usecase,
            delete_usecase,
            list_all_usecase,
            list_mine_usecase,
            get_by_id_usecase,
            resource_config,
        }
    }

    /// `create_reservation`のリソース解決（`view_submissions/reserve.rs`と同型のロジック）
    fn resolve_resources(&self, params: &CreateReservationParams) -> Result<Vec<Resource>, String> {
        match params.resource_type.as_str() {
            "gpu" => {
                let server_name = params
                    .server
                    .clone()
                    .ok_or("GPU予約にはserverの指定が必要です")?;
                let server_config = self
                    .resource_config
                    .get_server(&server_name)
                    .ok_or_else(|| format!("サーバー {} が見つかりません", server_name))?;

                let device_numbers = params.device_numbers.clone().unwrap_or_default();

                if device_numbers.is_empty() {
                    // デバイス未指定の場合はサーバーの全デバイスを対象にする
                    Ok(server_config
                        .devices
                        .iter()
                        .map(|device| {
                            Resource::Gpu(Gpu::new(
                                server_name.clone(),
                                device.id,
                                device.model.clone(),
                            ))
                        })
                        .collect())
                } else {
                    device_numbers
                        .iter()
                        .map(|device_id| {
                            server_config
                                .devices
                                .iter()
                                .find(|d| d.id == *device_id)
                                .map(|d| {
                                    Resource::Gpu(Gpu::new(
                                        server_name.clone(),
                                        d.id,
                                        d.model.clone(),
                                    ))
                                })
                                .ok_or_else(|| format!("デバイス {} が見つかりません", device_id))
                        })
                        .collect()
                }
            }
            "room" => {
                let room_name = params
                    .room
                    .clone()
                    .ok_or("部屋予約にはroomの指定が必要です")?;
                Ok(vec![Resource::Room { name: room_name }])
            }
            other => Err(format!("不明なresource_type: {}", other)),
        }
    }

    #[tool(description = "今後（進行中含む）の全ての予約を一覧表示する")]
    async fn list_all_reservations(&self) -> Result<CallToolResult, ErrorData> {
        match self.list_all_usecase.execute().await {
            Ok(usages) => {
                let dtos: Vec<ReservationDto> = usages.iter().map(ReservationDto::from).collect();
                Ok(success_json(&dtos))
            }
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分が所有する予約を一覧表示する")]
    async fn list_my_reservations(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        match self.list_mine_usecase.execute(&caller).await {
            Ok(usages) => {
                let dtos: Vec<ReservationDto> = usages.iter().map(ReservationDto::from).collect();
                Ok(success_json(&dtos))
            }
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "IDを指定して予約の詳細を取得する")]
    async fn get_reservation(
        &self,
        Parameters(params): Parameters<GetReservationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = UsageId::from_string(params.id);
        match self.get_by_id_usecase.execute(&id).await {
            Ok(usage) => Ok(success_json(&ReservationDto::from(&usage))),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "新しい予約を作成する（GPUサーバーまたは部屋）")]
    async fn create_reservation(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<CreateReservationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;

        let time_period = match TimePeriod::new(params.start, params.end) {
            Ok(time_period) => time_period,
            Err(e) => return Ok(tool_error(e)),
        };

        let resources = match self.resolve_resources(&params) {
            Ok(resources) => resources,
            Err(message) => return Ok(tool_error(message)),
        };

        match self
            .create_usecase
            .execute(caller, time_period, resources, params.notes)
            .await
        {
            Ok(usage_id) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "予約を作成しました。予約ID: {}",
                usage_id.as_str()
            ))])),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分が所有する予約の時間または備考を更新する")]
    async fn update_reservation(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<UpdateReservationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = UsageId::from_string(params.id);

        let time_period = match (params.start, params.end) {
            (Some(start), Some(end)) => match TimePeriod::new(start, end) {
                Ok(time_period) => Some(time_period),
                Err(e) => return Ok(tool_error(e)),
            },
            (None, None) => None,
            _ => {
                return Ok(tool_error(
                    "startとendは両方指定するか、両方省略してください",
                ));
            }
        };

        match self
            .update_usecase
            .execute(&id, &caller, time_period, params.notes)
            .await
        {
            Ok(()) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "予約を更新しました",
            )])),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分が所有する予約をキャンセルする")]
    async fn cancel_reservation(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<CancelReservationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = UsageId::from_string(params.id);

        match self.delete_usecase.execute(&id, &caller).await {
            Ok(()) => Ok(CallToolResult::success(vec![ContentBlock::text(
                "予約をキャンセルしました",
            )])),
            Err(e) => Ok(tool_error(e)),
        }
    }
}

#[tool_handler]
impl<R> ServerHandler for LrmMcpServer<R>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
{
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "lab-resource-manager の予約を閲覧・作成・更新・キャンセルするツール群。\
            書き込み系ツールは呼び出し元のBearerトークンに紐づくメールアドレスを所有者として扱う。"
                .to_string(),
        );
        info
    }
}
