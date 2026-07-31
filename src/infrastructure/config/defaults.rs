//! 設定のデフォルト値
//!
//! systemdデプロイ時の標準パスに基づく。

/// Google サービスアカウントJSONキーのデフォルトパス
pub const GOOGLE_SERVICE_ACCOUNT_KEY_PATH: &str = "/etc/lab-resource-manager/service-account.json";

/// リソース設定ファイルのデフォルトパス
pub const RESOURCE_CONFIG_PATH: &str = "/etc/lab-resource-manager/resources.toml";

/// ID紐付けファイルのデフォルトパス
pub const IDENTITY_LINKS_FILE: &str = "/var/lib/lab-resource-manager/identity_links.json";

/// カレンダーIDマッピングファイルのデフォルトパス
pub const CALENDAR_MAPPINGS_FILE: &str =
    "/var/lib/lab-resource-manager/google_calendar_mappings.json";

/// ポーリング間隔のデフォルト値（秒）
pub const POLLING_INTERVAL_SECS: u64 = 60;

/// GPU利用状況レポートを鮮度切れとみなす経過時間のデフォルト値（秒）
pub const GPU_USAGE_MAX_STALENESS_SECS: u64 = 300;

/// 未予約利用を提案対象とみなす継続時間の閾値のデフォルト値（秒）
pub const UNRESERVED_USAGE_THRESHOLD_SECS: u64 = 600;

/// 事後予約提案で提示する利用時間候補のデフォルト値（時間、カンマ区切り）
pub const RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS: &str = "1,2,3,5,8";

/// MCPアクセストークンファイルのデフォルトパス
pub const MCP_TOKENS_FILE: &str = "/var/lib/lab-resource-manager/mcp_tokens.json";

/// Web画面が時刻を表示するタイムゾーンのデフォルト値（IANAタイムゾーン名）
pub const WEB_TIMEZONE: &str = "Asia/Tokyo";
