use serde::Deserialize;
use std::collections::HashMap;

/// ストレージバックエンド固有の設定
///
/// リソース定義（「何が予約できるか」）から分離された設定。
/// 同じリソースセットでも異なるバックエンド（Google Calendar、別の永続化層など）に
/// 対応できるよう、実装詳細を別構造に移した。
#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    /// ストレージの種類と設定
    #[serde(flatten)]
    pub backend: StorageBackend,
}

/// ストレージバックエンド実装別の設定
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageBackend {
    /// Google Calendar バックエンド
    GoogleCalendar {
        /// リソース名からカレンダーIDへのマッピング
        ///
        /// キー: サーバー名または部屋名
        /// 値: Google Calendar ID (例: "xxx@group.calendar.google.com")
        calendars: HashMap<String, String>,
    },
}

impl StorageConfig {
    /// Google Calendar バックエンドのカレンダーIDマッピングを取得
    pub fn google_calendar_mappings(&self) -> Option<&HashMap<String, String>> {
        match &self.backend {
            StorageBackend::GoogleCalendar { calendars } => Some(calendars),
        }
    }

    /// リソース名のカレンダーIDを取得
    pub fn calendar_id_for_resource(&self, resource_name: &str) -> Option<&str> {
        match &self.backend {
            StorageBackend::GoogleCalendar { calendars } => {
                calendars.get(resource_name).map(|s| s.as_str())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_google_calendar_storage() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"Meeting Room A" = "yyy@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            config.backend,
            StorageBackend::GoogleCalendar { .. }
        ));

        assert_eq!(
            config.calendar_id_for_resource("gpu-server-1"),
            Some("xxx@group.calendar.google.com")
        );
        assert_eq!(
            config.calendar_id_for_resource("Meeting Room A"),
            Some("yyy@group.calendar.google.com")
        );
        assert_eq!(config.calendar_id_for_resource("unknown"), None);
    }

    #[test]
    fn get_calendar_mappings() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();
        let mappings = config.google_calendar_mappings().unwrap();

        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings.get("gpu-server-1").map(|s| s.as_str()),
            Some("xxx@group.calendar.google.com")
        );
    }
}
