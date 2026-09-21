use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::infrastructure::config::notification_format::{
    FormatConfig, NotificationCustomization, TemplateConfig,
};
use crate::infrastructure::config::storage_config::StorageConfig;
use serde::Deserialize;
use std::fs;

/// 通知設定の種類と設定値
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NotificationConfig {
    /// Slack通知設定
    Slack {
        /// Bot Token (xoxb-...)
        bot_token: String,
        /// チャンネルID (C01234567...)
        channel_id: String,
        /// タイムゾーン（オプション）
        #[serde(default)]
        timezone: Option<String>,
        /// メッセージテンプレート（オプション）
        #[serde(default)]
        templates: Option<TemplateConfig>,
        /// フォーマット設定（オプション）
        #[serde(default)]
        format: Option<FormatConfig>,
    },
    /// テスト/開発用モック通知設定
    Mock {
        /// タイムゾーン（オプション）
        #[serde(default)]
        timezone: Option<String>,
        /// メッセージテンプレート（オプション）
        #[serde(default)]
        templates: Option<TemplateConfig>,
        /// フォーマット設定（オプション）
        #[serde(default)]
        format: Option<FormatConfig>,
    },
}

impl NotificationConfig {
    /// タイムゾーン文字列を取得
    pub fn timezone(&self) -> Option<&str> {
        match self {
            NotificationConfig::Slack { timezone, .. } => timezone.as_deref(),
            NotificationConfig::Mock { timezone, .. } => timezone.as_deref(),
        }
    }

    /// カスタマイズ設定を取得（デフォルト値込み）
    pub fn customization(&self) -> NotificationCustomization {
        match self {
            NotificationConfig::Slack {
                templates, format, ..
            }
            | NotificationConfig::Mock {
                templates, format, ..
            } => NotificationCustomization {
                templates: templates.clone().unwrap_or_default(),
                format: format.clone().unwrap_or_default(),
            },
        }
    }
}

/// リソース全体の設定
///
/// 「何が予約できるか」を定義する設定。
/// ストレージバックエンド固有の設定（カレンダーIDなど）は分離された StorageConfig に移した。
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
    /// 通知設定のリスト
    pub notifications: Vec<NotificationConfig>,
}

impl ResourceConfig {
    /// サーバー設定を名前で検索
    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// 予約できるリソースを全て列挙する
    ///
    /// 設定に書かれた順（サーバーはデバイス番号順、その後に部屋）で返す。
    pub fn all_resources(&self) -> Vec<Resource> {
        let gpus = self.servers.iter().flat_map(|server| {
            server.devices.iter().map(move |device| {
                Resource::Gpu(Gpu::new(
                    server.name.clone(),
                    device.id,
                    device.model.clone(),
                ))
            })
        });

        let rooms = self.rooms.iter().map(|room| Resource::Room {
            name: room.name.clone(),
        });

        gpus.chain(rooms).collect()
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

/// リソースとストレージ設定の組み合わせ
///
/// resources.tomlから読み込まれた設定を分離するための中間構造。
#[derive(Debug, Deserialize)]
struct ConfigRoot {
    /// サーバー（GPU）の設定リスト
    #[serde(default)]
    servers: Vec<ServerConfig>,
    /// 部屋の設定リスト
    #[serde(default)]
    rooms: Vec<RoomConfig>,
    /// ストレージバックエンド設定
    #[serde(default)]
    storage: Option<StorageConfig>,
}

/// TOMLファイルからリソース設定を読み込む
///
/// `resources.toml` から ResourceConfig と StorageConfig を読み込み、タプルで返す。
pub fn load_config(
    path: impl AsRef<std::path::Path>,
) -> Result<(ResourceConfig, StorageConfig), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let root: ConfigRoot = toml::from_str(&content)?;

    let resource_config = ResourceConfig {
        servers: root.servers,
        rooms: root.rooms,
    };

    let storage_config = root.storage.ok_or("storage設定が見つかりません")?;

    Ok((resource_config, storage_config))
}
