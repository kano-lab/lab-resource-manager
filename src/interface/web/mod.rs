//! Web画面インターフェース層
//!
//! 予約をリソース×時間のタイムラインとして見せる閲覧専用の画面。予約の作成・変更・
//! キャンセルはSlackとMCPが担い、ここでは扱わない。
//!
//! 既存の`SlackApp`やMCPサーバーは変更せず、同一プロセス内で並行動作する独立した
//! HTTPリスナーとして追加する。Topcoatのルーターはパスの接頭辞でネストできないため、
//! MCPとはポートを分ける。
//!
//! 認証は行わない。`docs/ADMIN_GUIDE.md`が前提とする「研究室のLAN内でのみ到達できる」
//! 運用に依存している。閲覧専用で書き込みの経路がないこと、所有者はメールアドレスの
//! ローカルパートしか表示しないことで、露出する情報を抑えている。

pub mod page;
pub mod query;
pub mod timeline;
pub mod view;

use crate::infrastructure::config::ResourceConfig;
use chrono_tz::Tz;
use page::DisplayTimezone;
use query::ReservationQuery;
use std::net::SocketAddr;
use std::sync::Arc;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use tracing::info;

/// Web画面をHTTPで起動する
///
/// 画面が必要とする依存は`app_context`に登録する。Topcoatの`#[page]`はグローバル関数で
/// 型パラメータを持てないため、リポジトリ実装でジェネリックなユースケースは
/// [`ReservationQuery`]としてトレイトオブジェクトに均してから渡す。
///
/// サーバーがエラーで終了した場合にエラーを返す。
pub async fn serve(
    listen_addr: SocketAddr,
    reservations: Arc<dyn ReservationQuery>,
    resource_config: Arc<ResourceConfig>,
    timezone: Tz,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    serve_on(listener, reservations, resource_config, timezone).await
}

/// 待ち受け済みのリスナーでWeb画面を起動する
///
/// 空きポートをOSに選ばせたい場合（テストなど）に、呼び出し側が実際のアドレスを
/// 知れるようにするための入り口。
pub async fn serve_on(
    listener: tokio::net::TcpListener,
    reservations: Arc<dyn ReservationQuery>,
    resource_config: Arc<ResourceConfig>,
    timezone: Tz,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = Router::builder()
        .discover()
        .app_context(reservations)
        .app_context(resource_config)
        .app_context(DisplayTimezone(timezone))
        .build();

    let listen_addr = listener.local_addr()?;
    info!(listen_addr = %listen_addr, timezone = %timezone, "web server listening");

    topcoat::serve(listener, router).await?;

    Ok(())
}
