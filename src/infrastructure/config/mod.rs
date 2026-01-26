//! # Configuration Module
//!
//! このモジュールは、アプリケーションの設定ファイルの読み込みと管理を担当します。
//!
//! 設定は以下の3つの責務に分離されています：
//! - **構造** (`app_config`): 設定値の型定義
//! - **デフォルト値** (`defaults`): 各設定のデフォルト値
//! - **読み込み** (`loader`): 環境変数からの読み込みロジック

/// アプリケーション設定の構造定義
pub mod app_config;
/// 設定のデフォルト値
pub mod defaults;
/// 設定の読み込み
pub mod loader;
/// リソース設定の定義と読み込み
pub mod resource_config;

pub use app_config::AppConfig;
pub use loader::{load_from_env, ConfigLoadError};
pub use resource_config::{
    DeviceConfig, NotificationConfig, ResourceConfig, RoomConfig, ServerConfig, load_config,
};
