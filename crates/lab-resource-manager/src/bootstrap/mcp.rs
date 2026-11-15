//! MCPサーバーの組み立てと起動

use super::BoxError;
use super::services::Services;
use crate::application::usecases::{
    CancelReservationUseCase, GetReservationUseCase, GetReservedSlotUseCase,
    ListReservedSlotsHeldByUseCase, ListReservedSlotsUseCase,
};
use crate::infrastructure::config::AppConfig;
use crate::interface::mcp::{
    self,
    server::{LrmMcpServer, McpUseCases},
};
use axum_server::tls_rustls::RustlsConfig;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// MCPサーバーを始める（`MCP_LISTEN_ADDR`が未設定なら始めない）
///
/// # Errors
/// TLS証明書を読み込めない場合。壊れた証明書に気づかずHTTPへ静かに
/// 退くほうが危険なため、起動の時点で止める。
pub(super) async fn start(
    app_config: &AppConfig,
    services: &Services,
    shutdown: CancellationToken,
) -> Result<Option<JoinHandle<()>>, BoxError> {
    let Some(listen_addr) = app_config.mcp_listen_addr else {
        info!("mcp server disabled: MCP_LISTEN_ADDR is not set");
        return Ok(None);
    };

    let tls_config = load_tls(app_config).await?;
    let server = LrmMcpServer::new(usecases(services), services.resource_config.clone());
    let allowed_hosts = app_config.mcp_allowed_hosts.clone();
    let token_repo = services.mcp_token_repo.clone();

    info!(listen_addr = %listen_addr, "mcp server enabled");
    Ok(Some(tokio::spawn(async move {
        tokio::select! {
            result = mcp::serve(listen_addr, allowed_hosts, tls_config, token_repo, server) => {
                if let Err(e) = result {
                    error!(error = %e, "mcp server stopped with an error");
                }
            }
            _ = shutdown.cancelled() => {}
        }
    })))
}

/// TLS証明書を読み込む（証明書と鍵の両方があるか、どちらもないかはローダーが確かめている）
async fn load_tls(app_config: &AppConfig) -> Result<Option<RustlsConfig>, BoxError> {
    match (&app_config.mcp_tls_cert_file, &app_config.mcp_tls_key_file) {
        (Some(cert), Some(key)) => {
            Ok(Some(RustlsConfig::from_pem_file(cert, key).await.map_err(
                |e| format!("MCPサーバーのTLS証明書の読み込みに失敗: {}", e),
            )?))
        }
        _ => Ok(None),
    }
}

fn usecases(services: &Services) -> McpUseCases<super::services::Repository> {
    let repo = &services.reservation_repo;
    let reservations = &services.reservations;

    McpUseCases {
        create_reservation: reservations.create.clone(),
        get_reservation: Arc::new(GetReservationUseCase::new(repo.clone())),
        cancel_reservation: Arc::new(CancelReservationUseCase::new(repo.clone())),
        list_reserved_slots: Arc::new(ListReservedSlotsUseCase::new(repo.clone())),
        list_reserved_slots_held_by: Arc::new(ListReservedSlotsHeldByUseCase::new(repo.clone())),
        get_reserved_slot: Arc::new(GetReservedSlotUseCase::new(repo.clone())),
        update_reserved_slot: reservations.update_slot.clone(),
        end_reserved_slot_early: reservations.end_slot_early.clone(),
        cancel_reserved_slot: reservations.cancel_slot.clone(),
        check_resource_availability: reservations.check_availability.clone(),
    }
}
