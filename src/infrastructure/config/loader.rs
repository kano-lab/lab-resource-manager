//! 設定の読み込み
//!
//! 環境変数から設定を読み込むロジックを担当する。
//! 構造やデフォルト値の知識は別モジュールから取得する。

use super::app_config::AppConfig;
use super::defaults;
use chrono::Duration;
use std::env;
use std::net::SocketAddr;
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
    InvalidEnvVar { name: &'static str, reason: String },
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
            s.parse::<u64>()
                .map_err(|_| ConfigLoadError::InvalidEnvVar {
                    name: "POLLING_INTERVAL",
                    reason: "正の整数である必要があります".to_string(),
                })
        })
        .transpose()?
        .unwrap_or(defaults::POLLING_INTERVAL_SECS);

    let gpu_usage_reports_dir = env::var("GPU_USAGE_REPORTS_DIR").ok().map(PathBuf::from);

    let gpu_usage_max_staleness_secs = env::var("GPU_USAGE_MAX_STALENESS_SECS")
        .ok()
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| ConfigLoadError::InvalidEnvVar {
                    name: "GPU_USAGE_MAX_STALENESS_SECS",
                    reason: "正の整数である必要があります".to_string(),
                })
        })
        .transpose()?
        .unwrap_or(defaults::GPU_USAGE_MAX_STALENESS_SECS);

    let unreserved_usage_threshold_secs = env::var("UNRESERVED_USAGE_THRESHOLD_SECS")
        .ok()
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| ConfigLoadError::InvalidEnvVar {
                    name: "UNRESERVED_USAGE_THRESHOLD_SECS",
                    reason: "正の整数である必要があります".to_string(),
                })
        })
        .transpose()?
        .unwrap_or(defaults::UNRESERVED_USAGE_THRESHOLD_SECS);

    let reservation_proposal_duration_candidates =
        env::var("RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS")
            .ok()
            .map(|s| parse_duration_candidates_hours(&s))
            .transpose()?
            .unwrap_or(
                parse_duration_candidates_hours(
                    defaults::RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS,
                )
                .expect("デフォルト値は常にパース可能"),
            );

    let mcp_listen_addr = env::var("MCP_LISTEN_ADDR")
        .ok()
        .map(|s| {
            s.parse::<SocketAddr>()
                .map_err(|_| ConfigLoadError::InvalidEnvVar {
                    name: "MCP_LISTEN_ADDR",
                    reason: "\"host:port\"形式である必要があります".to_string(),
                })
        })
        .transpose()?;

    let mcp_tokens_file = env::var("MCP_TOKENS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(defaults::MCP_TOKENS_FILE));

    let mcp_allowed_hosts = env::var("MCP_ALLOWED_HOSTS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Ok(AppConfig {
        google_service_account_key_path,
        slack_bot_token,
        slack_app_token,
        resource_config_path,
        identity_links_file,
        calendar_mappings_file,
        polling_interval_secs,
        gpu_usage_reports_dir,
        gpu_usage_max_staleness_secs,
        unreserved_usage_threshold_secs,
        reservation_proposal_duration_candidates,
        mcp_listen_addr,
        mcp_tokens_file,
        mcp_allowed_hosts,
    })
}

/// カンマ区切りの時間数文字列を`Duration`のリストにパースする
///
/// 例: "1,2,3,5,8" -> `[Duration::hours(1), Duration::hours(2), ...]`
fn parse_duration_candidates_hours(raw: &str) -> Result<Vec<Duration>, ConfigLoadError> {
    let candidates: Result<Vec<Duration>, ConfigLoadError> = raw
        .split(',')
        .map(|s| {
            s.trim().parse::<i64>().map(Duration::hours).map_err(|_| {
                ConfigLoadError::InvalidEnvVar {
                    name: "RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS",
                    reason: format!("'{}'は正の整数である必要があります", s.trim()),
                }
            })
        })
        .collect();

    match candidates {
        Ok(c) if c.is_empty() => Err(ConfigLoadError::InvalidEnvVar {
            name: "RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS",
            reason: "少なくとも1つの候補が必要です".to_string(),
        }),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_candidates_hours_valid() {
        let result = parse_duration_candidates_hours("1,2,3,5,8").unwrap();
        assert_eq!(
            result,
            vec![
                Duration::hours(1),
                Duration::hours(2),
                Duration::hours(3),
                Duration::hours(5),
                Duration::hours(8),
            ]
        );
    }

    #[test]
    fn test_parse_duration_candidates_hours_trims_whitespace() {
        let result = parse_duration_candidates_hours(" 1 , 2 ").unwrap();
        assert_eq!(result, vec![Duration::hours(1), Duration::hours(2)]);
    }

    #[test]
    fn test_parse_duration_candidates_hours_single_value() {
        let result = parse_duration_candidates_hours("4").unwrap();
        assert_eq!(result, vec![Duration::hours(4)]);
    }

    #[test]
    fn test_parse_duration_candidates_hours_rejects_non_numeric() {
        let result = parse_duration_candidates_hours("1,abc,3");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duration_candidates_hours_rejects_empty_string() {
        let result = parse_duration_candidates_hours("");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_duration_candidates_are_parseable() {
        let result = parse_duration_candidates_hours(
            defaults::RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS,
        );
        assert!(result.is_ok());
    }
}
