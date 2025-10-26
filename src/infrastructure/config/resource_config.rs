use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

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
}

pub fn load_config(path: &str) -> Result<ResourceConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: ResourceConfig = toml::from_str(&content)?;
    Ok(config)
}
