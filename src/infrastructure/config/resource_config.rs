use crate::domain::aggregates::resource_usage::value_objects::Resource;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NotificationConfig {
    Slack {
        webhook_url: String,
        #[serde(default)]
        timezone: Option<String>,
    },
    Mock {
        #[serde(default)]
        timezone: Option<String>,
    },
}

impl NotificationConfig {
    /// Get the timezone string for this notification config, if any
    pub fn timezone(&self) -> Option<&str> {
        match self {
            NotificationConfig::Slack { timezone, .. } => timezone.as_deref(),
            NotificationConfig::Mock { timezone } => timezone.as_deref(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ResourceConfig {
    pub servers: Vec<ServerConfig>,
    pub rooms: Vec<RoomConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub calendar_id: String,
    pub devices: Vec<DeviceConfig>,
    pub notifications: Vec<NotificationConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    pub id: u32,
    pub model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoomConfig {
    pub name: String,
    pub calendar_id: String,
    pub notifications: Vec<NotificationConfig>,
}

impl ResourceConfig {
    pub fn calendar_to_server_map(&self) -> HashMap<String, String> {
        self.servers
            .iter()
            .map(|s| (s.calendar_id.clone(), s.name.clone()))
            .collect()
    }

    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }

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

pub fn load_config(path: &str) -> Result<ResourceConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: ResourceConfig = toml::from_str(&content)?;
    Ok(config)
}
