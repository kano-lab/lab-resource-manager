use crate::domain::aggregates::resource_usage::value_objects::Resource;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// 通知設定の種類と設定値
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NotificationConfig {
    /// Slack通知設定
    Slack {
        /// Webhook URL
        webhook_url: String,
        /// タイムゾーン（オプション）
        #[serde(default)]
        timezone: Option<String>,
    },
    /// テスト/開発用モック通知設定
    Mock {
        /// タイムゾーン（オプション）
        #[serde(default)]
        timezone: Option<String>,
    },
}

impl NotificationConfig {
    /// タイムゾーン文字列を取得
    pub fn timezone(&self) -> Option<&str> {
        match self {
            NotificationConfig::Slack { timezone, .. } => timezone.as_deref(),
            NotificationConfig::Mock { timezone } => timezone.as_deref(),
        }
    }
}

/// リソース全体の設定
#[derive(Debug, Deserialize, Clone)]
pub struct ResourceConfig {
    /// サーバー（GPU）の設定リスト
    pub servers: Vec<ServerConfig>,
    /// 部屋の設定リスト
    pub rooms: Vec<RoomConfig>,
}

/// サーバー（GPU）の設定
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// サーバー名
    pub name: String,
    /// カレンダーID
    pub calendar_id: String,
    /// デバイス（GPU）のリスト
    pub devices: Vec<DeviceConfig>,
    /// 通知設定のリスト
    pub notifications: Vec<NotificationConfig>,
}

/// デバイス（GPU）の設定
#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    /// デバイスID
    pub id: u32,
    /// モデル名
    pub model: String,
}

/// 部屋の設定
#[derive(Debug, Deserialize, Clone)]
pub struct RoomConfig {
    /// 部屋名
    pub name: String,
    /// カレンダーID
    pub calendar_id: String,
    /// 通知設定のリスト
    pub notifications: Vec<NotificationConfig>,
}

impl ResourceConfig {
    /// カレンダーIDからサーバー名へのマッピングを取得
    pub fn calendar_to_server_map(&self) -> HashMap<String, String> {
        self.servers
            .iter()
            .map(|s| (s.calendar_id.clone(), s.name.clone()))
            .collect()
    }

    /// サーバー設定を名前で検索
    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// リソースに対する通知設定を取得
    pub fn get_notifications_for_resource(&self, resource: &Resource) -> Vec<NotificationConfig> {
        match resource {
            Resource::Gpu(gpu) => self
                .servers
                .iter()
                .find(|s| s.name == gpu.server())
                .map(|s| s.notifications.clone())
                .unwrap_or_default(),
            Resource::Room { name } => self
                .rooms
                .iter()
                .find(|r| r.name == *name)
                .map(|r| r.notifications.clone())
                .unwrap_or_default(),
        }
    }
}

/// TOMLファイルからリソース設定を読み込む
pub fn load_config(path: &str) -> Result<ResourceConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: ResourceConfig = toml::from_str(&content)?;
    Ok(config)
}
