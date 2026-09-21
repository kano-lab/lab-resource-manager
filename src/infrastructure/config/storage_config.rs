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

    /// 指定されたリソース名（サーバーと部屋の合計）がすべてストレージマッピングに存在することを検証
    ///
    /// # 返り値
    /// - `Ok(())`: すべてのリソースが写像を持つ場合
    /// - `Err(missing_resources)`: 写像のないリソース名のリスト
    pub fn validate_resource_mappings(
        &self,
        server_names: &[String],
        room_names: &[String],
    ) -> Result<(), Vec<String>> {
        let mappings = match self.google_calendar_mappings() {
            Some(m) => m,
            None => return Err(vec!["storage.calendars マップが見つかりません".to_string()]),
        };

        let mut missing = Vec::new();

        for server_name in server_names {
            if !mappings.contains_key(server_name) {
                missing.push(format!("サーバー '{}'", server_name));
            }
        }

        for room_name in room_names {
            if !mappings.contains_key(room_name) {
                missing.push(format!("部屋 '{}'", room_name));
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// ストレージマッピングに存在するが、リソース定義に存在しないキーを検出
    ///
    /// これは警告対象です（リソース追加前の事前登録がありうるため、エラーではない）
    pub fn find_unmapped_calendars(
        &self,
        server_names: &[String],
        room_names: &[String],
    ) -> Vec<String> {
        let mappings = match self.google_calendar_mappings() {
            Some(m) => m,
            None => return Vec::new(),
        };

        let all_resource_names: std::collections::HashSet<_> = server_names
            .iter()
            .chain(room_names.iter())
            .map(|s| s.as_str())
            .collect();

        mappings
            .keys()
            .filter(|key| !all_resource_names.contains(key.as_str()))
            .cloned()
            .collect()
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

    #[test]
    fn validate_all_resources_have_mappings() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"Meeting Room A" = "yyy@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();

        let servers = vec!["gpu-server-1".to_string()];
        let rooms = vec!["Meeting Room A".to_string()];

        assert!(config.validate_resource_mappings(&servers, &rooms).is_ok());
    }

    #[test]
    fn validate_missing_server_mapping() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();

        let servers = vec!["gpu-server-1".to_string(), "gpu-server-2".to_string()];
        let rooms = vec![];

        let result = config.validate_resource_mappings(&servers, &rooms);
        assert!(result.is_err());
        let missing = result.unwrap_err();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("gpu-server-2"));
    }

    #[test]
    fn validate_missing_room_mapping() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"Meeting Room A" = "xxx@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();

        let servers = vec![];
        let rooms = vec!["Meeting Room A".to_string(), "Meeting Room B".to_string()];

        let result = config.validate_resource_mappings(&servers, &rooms);
        assert!(result.is_err());
        let missing = result.unwrap_err();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("Meeting Room B"));
    }

    #[test]
    fn find_unmapped_calendars_returns_extra_keys() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"gpu-server-2" = "yyy@group.calendar.google.com"
"Meeting Room A" = "zzz@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();

        let servers = vec!["gpu-server-1".to_string()];
        let rooms = vec![];

        let unmapped = config.find_unmapped_calendars(&servers, &rooms);
        assert_eq!(unmapped.len(), 2);
        assert!(unmapped.iter().any(|k| k == "gpu-server-2"));
        assert!(unmapped.iter().any(|k| k == "Meeting Room A"));
    }

    #[test]
    fn find_unmapped_calendars_returns_empty_when_all_mapped() {
        let toml_str = r#"
type = "google_calendar"

[calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"Meeting Room A" = "yyy@group.calendar.google.com"
        "#;

        let config: StorageConfig = toml::from_str(toml_str).unwrap();

        let servers = vec!["gpu-server-1".to_string()];
        let rooms = vec!["Meeting Room A".to_string()];

        let unmapped = config.find_unmapped_calendars(&servers, &rooms);
        assert!(unmapped.is_empty());
    }
}
