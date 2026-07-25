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
/// `allowed_hosts`が空の場合はHostヘッダ検証を無効化する（警告ログを出力）。
/// `axum::serve`がエラーで終了した場合にエラーを返す。
pub async fn serve<R>(
    listen_addr: SocketAddr,
    allowed_hosts: Vec<String>,
    mcp_token_repo: Arc<dyn McpTokenRepository>,
    server: LrmMcpServer<R>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
{
    let session_manager = Arc::new(LocalSessionManager::default());

    let config = if allowed_hosts.is_empty() {
        warn!("⚠️ MCP_ALLOWED_HOSTS未設定のため、MCPサーバーのHostヘッダ検証を無効化します");
        StreamableHttpServerConfig::default().disable_allowed_hosts()
    } else {
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts)
    };

    let service = StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(mcp_token_repo, bearer_auth));

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    info!("🔌 MCPサーバーをリッスンしています: {}", listen_addr);

    axum::serve(listener, router).await?;

    Ok(())
}
