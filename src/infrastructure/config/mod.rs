//! # Configuration Module
//!
//! このモジュールは、アプリケーションの設定ファイルの読み込みと管理を担当します。

/// リソース設定の定義と読み込み
pub mod resource_config;

pub use resource_config::{
    DeviceConfig, NotificationConfig, ResourceConfig, RoomConfig, ServerConfig, load_config,
};
