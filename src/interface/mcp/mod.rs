//! MCP（Model Context Protocol）インターフェース層
//!
//! Claude Codeなどのエージェントから、既存のユースケース層を経由してLRMの予約を
//! 閲覧・作成・キャンセルできるようにする。既存の`SlackApp`は変更せず、
//! 同一プロセス内で並行動作する独立したHTTP/SSEリスナーとして追加する。
//!
//! 認証は`Authorization: Bearer <token>`ヘッダで行い、トークンは`/mcp-token`
//! Slashコマンドで発行される（[`crate::domain::ports::repositories::McpTokenRepository`]）。

pub mod auth;
pub mod dto;
pub mod server;

use crate::domain::ports::repositories::{McpTokenRepository, ResourceUsageRepository};
use auth::bearer_auth;
use axum::middleware;
use axum_server::tls_rustls::RustlsConfig;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use server::LrmMcpServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

/// MCPサーバーをHTTP/SSEで起動する
///
/// `allowed_hosts`は呼び出し側(設定ローダー)でMCP機能有効時は非空であることが
/// 保証されている前提。`tls_config`が`Some`ならHTTPS、`None`ならHTTPで待ち受ける
/// (TLSは推奨だが必須ではない)。サーバーがエラーで終了した場合にエラーを返す。
pub async fn serve<R>(
    listen_addr: SocketAddr,
    allowed_hosts: Vec<String>,
    tls_config: Option<RustlsConfig>,
    mcp_token_repo: Arc<dyn McpTokenRepository>,
    server: LrmMcpServer<R>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
{
    let session_manager = Arc::new(LocalSessionManager::default());
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts);

    let service = StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(mcp_token_repo, bearer_auth));

    if let Some(tls_config) = tls_config {
        info!(listen_addr = %listen_addr, tls = true, "mcp server listening");
        axum_server::bind_rustls(listen_addr, tls_config)
            .serve(router.into_make_service())
            .await?;
    } else {
        warn!(
            "⚠️ MCPサーバーをHTTPで起動しています(TLS未設定)。Bearerトークンが平文で送信されます。\
            MCP_TLS_CERT_FILE/MCP_TLS_KEY_FILEの設定を推奨します"
        );
        let listener = tokio::net::TcpListener::bind(listen_addr).await?;
        info!(listen_addr = %listen_addr, tls = false, "mcp server listening");
        axum::serve(listener, router).await?;
    }

    Ok(())
}
