//! LRM MCPサーバー
//!
//! 既存のユースケース層をそのまま呼び出す薄いアダプタ。新しい業務ロジックは持たない。
//!
//! ツールの名前はユビキタス言語に従う。予約は作る・取り消すの単位、
//! 予約枠は予約が実際に押さえている時間で、一覧・更新・今で終了は予約枠に対して行う。

use crate::application::usecases::{
    CancelReservationUseCase, CancelSlotUseCase, CheckResourceAvailabilityUseCase,
    CreateReservationUseCase, EndSlotEarlyUseCase, GetReservationUseCase, GetSlotUseCase,
    ListSlotsHeldByUseCase, ListSlotsUseCase, UpdateSlotUseCase,
};
use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource, TimePeriod};
use crate::domain::aggregates::reservation::{ReservationId, ReservedSlotId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ReservationRepository;
use crate::infrastructure::config::ResourceConfig;
use crate::interface::mcp::auth::ResolvedCaller;
use crate::interface::mcp::dto::{
    CheckAvailabilityParams, CreateReservationParams, ReservationDto, ReservationIdParams,
    ResourceAvailabilityDto, ResourceCatalogDto, SlotDto, SlotIdParams, UpdateSlotParams,
    resources_to_check,
};
use rmcp::ErrorData;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use std::sync::Arc;

/// MCPサーバーが呼び出すユースケース
pub struct McpUseCases<R: ReservationRepository> {
    /// 単発予約を作る
    pub create_reservation: Arc<CreateReservationUseCase<R>>,
    /// 予約を取得する
    pub get_reservation: Arc<GetReservationUseCase<R>>,
    /// 予約を取り消す
    pub cancel_reservation: Arc<CancelReservationUseCase<R>>,
    /// 指定期間の予約枠を一覧する
    pub list_slots: Arc<ListSlotsUseCase<R>>,
    /// 予約者の予約枠を一覧する
    pub list_slots_held_by: Arc<ListSlotsHeldByUseCase<R>>,
    /// 予約枠を取得する
    pub get_slot: Arc<GetSlotUseCase<R>>,
    /// 予約枠の期間・備考を更新する
    pub update_slot: Arc<UpdateSlotUseCase<R>>,
    /// 進行中の予約枠を今で終了する
    pub end_slot_early: Arc<EndSlotEarlyUseCase<R>>,
    /// 予約枠を取り消す
    pub cancel_slot: Arc<CancelSlotUseCase<R>>,
    /// リソースの空きを調べる
    pub check_resource_availability: Arc<CheckResourceAvailabilityUseCase<R>>,
}

/// LRMの予約をMCPツールとして公開するサーバー
///
/// 全フィールドが`Arc`のため安価に`Clone`でき、接続ごとに新しいインスタンスを渡せる。
/// `R: Clone`を要求しないよう`Clone`は手動実装する（`derive(Clone)`は`R: Clone`を要求してしまう）。
pub struct LrmMcpServer<R: ReservationRepository> {
    usecases: Arc<McpUseCases<R>>,
    resource_config: Arc<ResourceConfig>,
}

impl<R: ReservationRepository> Clone for LrmMcpServer<R> {
    fn clone(&self) -> Self {
        Self {
            usecases: self.usecases.clone(),
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

/// 予約枠の一覧が対象とする期間の長さ
///
/// カレンダーの繰り返しの予定は個々の予約枠へ展開されるため、上限を設けないと数十年先の
/// 予約枠まで返る。エージェントが把握したいのは手近な予約枠なので、ここで区切る。
const LISTING_WINDOW_DAYS: i64 = 60;

/// 一覧の対象期間（現在時刻から`LISTING_WINDOW_DAYS`日先まで）
fn listing_window()
-> Result<TimePeriod, crate::domain::aggregates::reservation::errors::ReservationError> {
    let now = chrono::Utc::now();
    TimePeriod::new(now, now + chrono::Duration::days(LISTING_WINDOW_DAYS))
}

/// 空きを一度に調べられる期間の上限
///
/// カレンダーの繰り返しの予定は個々の予約枠へ展開されるため、上限を設けないと
/// 際限なく予約枠を読むことになる。
const AVAILABILITY_WINDOW_MAX_DAYS: i64 = 60;

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

fn success_text(message: &str) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(message)])
}

#[tool_router]
impl<R> LrmMcpServer<R>
where
    R: ReservationRepository + Send + Sync + 'static,
{
    /// 新しいLrmMcpServerを作成
    pub fn new(usecases: McpUseCases<R>, resource_config: Arc<ResourceConfig>) -> Self {
        Self {
            usecases: Arc::new(usecases),
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

    #[tool(
        description = "予約できるリソース（GPUサーバーとそのデバイス番号・モデル、部屋）を一覧表示する。create_reservationに指定できる名前と番号はここで確認できる"
    )]
    async fn list_resources(&self) -> Result<CallToolResult, ErrorData> {
        Ok(success_json(&ResourceCatalogDto::from(
            self.resource_config.as_ref(),
        )))
    }

    #[tool(
        description = "指定した期間における各リソースの空きを調べる。リソースごとに空いている区間と、予約枠で埋まっている区間（予約者つき）を返す。serverまたはroomで絞り込める（省略時は全リソース）。調べられる期間は最長60日。予約されないまま使われているリソースは空きとして現れる"
    )]
    async fn check_availability(
        &self,
        Parameters(params): Parameters<CheckAvailabilityParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let window = match TimePeriod::new(params.start, params.end) {
            Ok(window) => window,
            Err(e) => return Ok(tool_error(e)),
        };
        if params.end - params.start > chrono::Duration::days(AVAILABILITY_WINDOW_MAX_DAYS) {
            return Ok(tool_error(format!(
                "調べられる期間は最長{AVAILABILITY_WINDOW_MAX_DAYS}日です。期間を短くしてください"
            )));
        }

        let resources = match resources_to_check(
            &self.resource_config,
            params.server.as_deref(),
            params.room.as_deref(),
        ) {
            Ok(resources) => resources,
            Err(message) => return Ok(tool_error(message)),
        };

        match self
            .usecases
            .check_resource_availability
            .execute(&resources, &window)
            .await
        {
            Ok(availabilities) => {
                let dtos: Vec<ResourceAvailabilityDto> = availabilities
                    .iter()
                    .map(ResourceAvailabilityDto::from)
                    .collect();
                Ok(success_json(&dtos))
            }
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "今後60日に重なる（進行中を含む）すべての予約枠を開始順に一覧表示する")]
    async fn list_slots(&self) -> Result<CallToolResult, ErrorData> {
        let window = match listing_window() {
            Ok(window) => window,
            Err(e) => return Ok(tool_error(e)),
        };
        match self.usecases.list_slots.execute(&window).await {
            Ok(slots) => Ok(success_json(
                &slots.iter().map(SlotDto::from).collect::<Vec<_>>(),
            )),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "今後60日に重なる（進行中を含む）自分の予約枠を開始順に一覧表示する")]
    async fn list_my_slots(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let window = match listing_window() {
            Ok(window) => window,
            Err(e) => return Ok(tool_error(e)),
        };
        match self
            .usecases
            .list_slots_held_by
            .execute(&caller, &window)
            .await
        {
            Ok(slots) => Ok(success_json(
                &slots.iter().map(SlotDto::from).collect::<Vec<_>>(),
            )),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "予約枠IDを指定して予約枠の詳細を取得する")]
    async fn get_slot(
        &self,
        Parameters(params): Parameters<SlotIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = ReservedSlotId::from_string(params.id);
        match self.usecases.get_slot.execute(&id).await {
            Ok(slot) => Ok(success_json(&SlotDto::from(&slot))),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "予約IDを指定して予約の詳細を取得する")]
    async fn get_reservation(
        &self,
        Parameters(params): Parameters<ReservationIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = ReservationId::from_string(params.id);
        match self.usecases.get_reservation.execute(&id).await {
            Ok(reservation) => Ok(success_json(&ReservationDto::from(&reservation))),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "単発予約を作る（GPUサーバーまたは部屋）")]
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
            .usecases
            .create_reservation
            .execute(caller, time_period, resources, params.notes)
            .await
        {
            Ok(reservation_id) => Ok(success_text(&format!(
                "単発予約を作りました。予約ID（予約枠IDと同じ）: {}",
                reservation_id.as_str()
            ))),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分の予約枠の期間または備考を変更する")]
    async fn update_slot(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<UpdateSlotParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = ReservedSlotId::from_string(params.id);

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
            .usecases
            .update_slot
            .execute(&id, &caller, time_period, params.notes)
            .await
        {
            Ok(()) => Ok(success_text("予約枠を変更しました")),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(
        description = "進行中の自分の予約枠を今の時点で終了し、残りの時間を空きに戻す。使った分は予約枠として残るため、無かったことにしたい場合はcancel_slotを使う"
    )]
    async fn end_slot_early(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<SlotIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = ReservedSlotId::from_string(params.id);

        match self.usecases.end_slot_early.execute(&id, &caller).await {
            Ok(slot) => Ok(success_json(&SlotDto::from(&slot))),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分の予約枠を1つ取り消す。単発予約の予約枠なら、その単発予約も無くなる")]
    async fn cancel_slot(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<SlotIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = ReservedSlotId::from_string(params.id);

        match self.usecases.cancel_slot.execute(&id, &caller).await {
            Ok(()) => Ok(success_text("予約枠を取り消しました")),
            Err(e) => Ok(tool_error(e)),
        }
    }

    #[tool(description = "自分の予約を取り消す。その予約の予約枠も無くなる")]
    async fn cancel_reservation(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<ReservationIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = resolved_caller(&parts)?;
        let id = ReservationId::from_string(params.id);

        match self.usecases.cancel_reservation.execute(&id, &caller).await {
            Ok(()) => Ok(success_text("予約を取り消しました")),
            Err(e) => Ok(tool_error(e)),
        }
    }
}

#[tool_handler]
impl<R> ServerHandler for LrmMcpServer<R>
where
    R: ReservationRepository + Send + Sync + 'static,
{
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "lab-resource-manager の予約を扱うツール群。予約は作る・取り消すの単位、\
            予約枠は予約が実際に押さえている時間で、単発予約は予約枠を1つ持つ。\
            一覧・変更・今で終了は予約枠に対して行う。予約できるリソースとその空きも調べられる。\
            書き込み系ツールは呼び出し元のBearerトークンに紐づくメールアドレスを予約者として扱う。"
                .to_string(),
        );
        info
    }
}
