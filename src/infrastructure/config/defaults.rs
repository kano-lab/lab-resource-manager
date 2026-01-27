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
