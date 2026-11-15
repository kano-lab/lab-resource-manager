//! MCPサーバーのBearerトークン認証ミドルウェア

use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::McpTokenRepository;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use tracing::error;

/// Bearerトークンから解決された呼び出し元のメールアドレス
///
/// 認証ミドルウェアがリクエストの`extensions`に挿入し、
/// 各MCPツールハンドラが`Extension<http::request::Parts>`経由で参照する。
#[derive(Debug, Clone)]
pub struct ResolvedCaller(pub EmailAddress);

/// `Authorization: Bearer <token>`を検証し、解決したメールアドレスを
/// リクエストの`extensions`に挿入してから後続に処理を渡す。
/// トークンが無い・無効な場合は401を返す。
pub async fn bearer_auth(
    State(mcp_token_repo): State<Arc<dyn McpTokenRepository>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let caller = mcp_token_repo
        .resolve(token)
        .await
        .map_err(|e| {
            error!(error = %e, "resolving the mcp token failed");
            StatusCode::UNAUTHORIZED
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(ResolvedCaller(caller));

    Ok(next.run(req).await)
}
