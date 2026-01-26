//! 設定の読み込み
//!
//! 環境変数から設定を読み込むロジックを担当する。
//! 構造やデフォルト値の知識は別モジュールから取得する。

use super::app_config::AppConfig;
use super::defaults;
use std::env;
use std::path::PathBuf;
use thiserror::Error;

/// 設定読み込み時のエラー
#[derive(Debug, Error)]
pub enum ConfigLoadError {
    /// 必須の環境変数が設定されていない
    #[error("環境変数 {0} が必要です")]
    MissingEnvVar(&'static str),
    /// 環境変数の値が不正
    #[error("環境変数 {name} の値が不正です: {reason}")]
    InvalidEnvVar {
        name: &'static str,
        reason: String,
    },
}

/// 環境変数から設定を読み込む
pub fn load_from_env() -> Result<AppConfig, ConfigLoadError> {
    let google_service_account_key_path = env::var("GOOGLE_SERVICE_ACCOUNT_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(defaults::GOOGLE_SERVICE_ACCOUNT_KEY_PATH));

    let slack_bot_token = env::var("SLACK_BOT_TOKEN")
        .map_err(|_| ConfigLoadError::MissingEnvVar("SLACK_BOT_TOKEN"))?;

    let slack_app_token = env::var("SLACK_APP_TOKEN")
        .map_err(|_| ConfigLoadError::MissingEnvVar("SLACK_APP_TOKEN"))?;

    let resource_config_path = env::var("RESOURCE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(defaults::RESOURCE_CONFIG_PATH));

    let identity_links_file = env::var("IDENTITY_LINKS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(defaults::IDENTITY_LINKS_FILE));

    let calendar_mappings_file = env::var("GOOGLE_CALENDAR_MAPPINGS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(defaults::CALENDAR_MAPPINGS_FILE));

    let polling_interval_secs = env::var("POLLING_INTERVAL")
        .ok()
        .map(|s| {
            s.parse::<u64>().map_err(|_| ConfigLoadError::InvalidEnvVar {
                name: "POLLING_INTERVAL",
                reason: "正の整数である必要があります".to_string(),
            })
        })
        .transpose()?
        .unwrap_or(defaults::POLLING_INTERVAL_SECS);

    Ok(AppConfig {
        google_service_account_key_path,
        slack_bot_token,
        slack_app_token,
        resource_config_path,
        identity_links_file,
        calendar_mappings_file,
        polling_interval_secs,
    })
}
