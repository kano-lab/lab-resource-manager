//! # Configuration Module
//!
//! このモジュールは、アプリケーションの設定ファイルの読み込みと管理を担当します。
pub mod resource_config;

pub use resource_config::{
    DeviceConfig, NotificationConfig, ResourceConfig, RoomConfig, ServerConfig, load_config,
};
