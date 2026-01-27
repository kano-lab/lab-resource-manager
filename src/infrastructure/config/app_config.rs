//! アプリケーション設定の構造定義
//!
//! このモジュールは設定値の型定義のみを担当し、
//! デフォルト値や読み込み方法は別モジュールで定義される。

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
}
