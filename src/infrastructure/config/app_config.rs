//! アプリケーション設定の構造定義
//!
//! このモジュールは設定値の型定義のみを担当し、
//! デフォルト値や読み込み方法は別モジュールで定義される。

use crate::application::usecases::detect_idle_reservations::NoticePolicy;
use chrono::Duration;
use std::net::SocketAddr;
use std::path::PathBuf;

/// アプリケーション全体の設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Google サービスアカウントJSONキーのパス
    pub google_service_account_key_path: PathBuf,
    /// Slack Bot User OAuth Token (xoxb-...)
    pub slack_bot_token: String,
    /// Socket Mode用のSlack App-Level Token (xapp-...)
    pub slack_app_token: String,
    /// リソース設定ファイルのパス
    pub resource_config_path: PathBuf,
    /// ID紐付けファイルのパス
    pub identity_links_file: PathBuf,
    /// カレンダーIDマッピングファイルのパス
    pub calendar_mappings_file: PathBuf,
    /// ポーリング間隔（秒）
    pub polling_interval_secs: u64,
    /// GPU利用状況レポートの共有ディレクトリ（未設定なら実利用観測機能を無効化）
    pub gpu_usage_reports_dir: Option<PathBuf>,
    /// GPU利用状況レポートを鮮度切れとみなす経過時間（秒）
    pub gpu_usage_max_staleness_secs: u64,
    /// 未予約利用を提案対象とみなす継続時間の閾値（秒）
    pub unreserved_usage_threshold_secs: u64,
    /// 事後予約提案で提示する利用時間の候補
    pub reservation_proposal_duration_candidates: Vec<Duration>,
    /// 予約者本人のプロセスを観測できない予約を知らせるまでの時間（秒）
    ///
    /// 残り時間がこれに満たない予約には知らせない（急かしても開けられる時間がないため）。
    pub idle_reservation_threshold_secs: u64,
    /// GPUを押さえたまま計算が走らない予約を知らせるまでの時間（秒）
    pub idle_held_gpu_threshold_secs: u64,
    /// 計算が走っているとみなす稼働率（%）
    pub computing_gpu_utilization_percent: u32,
    /// GPUを押さえたまま計算が走らない予約を、予約者に知らせるか様子を見るだけにするか
    pub idle_held_gpu_notices: NoticePolicy,
    /// 一度声をかけてから、次に声をかけるまで置く時間（秒）
    pub idle_notice_silence_secs: u64,
    /// MCPサーバーのHTTP/SSEリッスンアドレス（未設定ならMCP機能を無効化）
    pub mcp_listen_addr: Option<SocketAddr>,
    /// MCPアクセストークンファイルのパス
    pub mcp_tokens_file: PathBuf,
    /// MCPサーバーへのリクエストで許可するHostヘッダの値
    ///
    /// MCP機能が有効（`mcp_listen_addr`が`Some`）な場合は必ず1つ以上を含む
    /// （ローダーがfail-closedで検証する）。MCP機能が無効なら空。
    pub mcp_allowed_hosts: Vec<String>,
    /// MCPサーバーのTLS証明書ファイルのパス（PEM形式、未設定ならTLS無効）
    pub mcp_tls_cert_file: Option<PathBuf>,
    /// MCPサーバーのTLS秘密鍵ファイルのパス（PEM形式、未設定ならTLS無効）
    pub mcp_tls_key_file: Option<PathBuf>,
}
