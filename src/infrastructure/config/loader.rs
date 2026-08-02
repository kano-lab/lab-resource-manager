//! 設定の読み込み
//!
//! 環境変数から設定を読み込むロジックを担当する。
//! 構造やデフォルト値の知識は別モジュールから取得する。

use super::app_config::AppConfig;
use super::defaults;
use crate::application::usecases::detect_idle_reservations::NoticePolicy;
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

    let polling_interval_secs =
        parse_number_env("POLLING_INTERVAL", defaults::POLLING_INTERVAL_SECS)?;

    let gpu_usage_reports_dir = env::var("GPU_USAGE_REPORTS_DIR").ok().map(PathBuf::from);

    let gpu_usage_max_staleness_secs = parse_number_env(
        "GPU_USAGE_MAX_STALENESS_SECS",
        defaults::GPU_USAGE_MAX_STALENESS_SECS,
    )?;

    let unreserved_usage_threshold_secs = parse_number_env(
        "UNRESERVED_USAGE_THRESHOLD_SECS",
        defaults::UNRESERVED_USAGE_THRESHOLD_SECS,
    )?;

    let idle_reservation_threshold_secs = parse_number_env(
        "IDLE_RESERVATION_THRESHOLD_SECS",
        defaults::IDLE_RESERVATION_THRESHOLD_SECS,
    )?;

    let idle_held_gpu_threshold_secs = parse_number_env(
        "IDLE_HELD_GPU_THRESHOLD_SECS",
        defaults::IDLE_HELD_GPU_THRESHOLD_SECS,
    )?;

    let computing_gpu_utilization_percent = validate_percentage(
        "COMPUTING_GPU_UTILIZATION_PERCENT",
        parse_number_env(
            "COMPUTING_GPU_UTILIZATION_PERCENT",
            defaults::COMPUTING_GPU_UTILIZATION_PERCENT,
        )?,
    )?;

    let idle_held_gpu_notices = parse_notice_policy(
        "IDLE_HELD_GPU_NOTICES",
        env::var("IDLE_HELD_GPU_NOTICES")
            .ok()
            .as_deref()
            .unwrap_or(defaults::IDLE_HELD_GPU_NOTICES),
    )?;

    let idle_notice_silence_secs = parse_number_env(
        "IDLE_NOTICE_SILENCE_SECS",
        defaults::IDLE_NOTICE_SILENCE_SECS,
    )?;

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

    // MCP機能が有効(MCP_LISTEN_ADDR設定済み)なら、Hostヘッダ許可リストの明示設定を必須にする
    // (fail-closed。未設定のまま警告ログのみで起動を許すと、設定し忘れに気づけない)
    let mcp_allowed_hosts = validate_mcp_allowed_hosts(
        mcp_listen_addr,
        env::var("MCP_ALLOWED_HOSTS").ok().as_deref(),
    )?;

    let mcp_tls_cert_file = env::var("MCP_TLS_CERT_FILE").ok().map(PathBuf::from);
    let mcp_tls_key_file = env::var("MCP_TLS_KEY_FILE").ok().map(PathBuf::from);
    validate_mcp_tls_paths(&mcp_tls_cert_file, &mcp_tls_key_file)?;

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
        idle_reservation_threshold_secs,
        idle_held_gpu_threshold_secs,
        computing_gpu_utilization_percent,
        idle_held_gpu_notices,
        idle_notice_silence_secs,
        mcp_listen_addr,
        mcp_tokens_file,
        mcp_allowed_hosts,
        mcp_tls_cert_file,
        mcp_tls_key_file,
    })
}

/// 環境変数を数値として読む（未設定ならデフォルト値）
fn parse_number_env<T>(name: &'static str, default: T) -> Result<T, ConfigLoadError>
where
    T: std::str::FromStr,
{
    env::var(name)
        .ok()
        .map(|raw| {
            raw.trim()
                .parse::<T>()
                .map_err(|_| ConfigLoadError::InvalidEnvVar {
                    name,
                    reason: "正の整数である必要があります".to_string(),
                })
        })
        .transpose()
        .map(|parsed| parsed.unwrap_or(default))
}

/// 知らせるか様子を見るかの指定を読む
fn parse_notice_policy(name: &'static str, raw: &str) -> Result<NoticePolicy, ConfigLoadError> {
    match raw.trim() {
        "observe" => Ok(NoticePolicy::Observe),
        "notify" => Ok(NoticePolicy::Notify),
        other => Err(ConfigLoadError::InvalidEnvVar {
            name,
            reason: format!("'{other}'ではなく、observeかnotifyを指定してください"),
        }),
    }
}

/// 百分率として意味を持つ範囲に収まっていることを確かめる
fn validate_percentage(name: &'static str, value: u32) -> Result<u32, ConfigLoadError> {
    if value > 100 {
        return Err(ConfigLoadError::InvalidEnvVar {
            name,
            reason: "0から100までの値である必要があります".to_string(),
        });
    }
    Ok(value)
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

/// MCP機能有効時のHostヘッダ許可リストを検証する(fail-closed)
///
/// `mcp_listen_addr`が`Some`(MCP機能有効)のときは`raw`(カンマ区切り)から
/// 少なくとも1つのホストが得られることを要求する。`None`のときは検証をスキップし
/// 空リストを返す。
fn validate_mcp_allowed_hosts(
    mcp_listen_addr: Option<SocketAddr>,
    raw: Option<&str>,
) -> Result<Vec<String>, ConfigLoadError> {
    if mcp_listen_addr.is_none() {
        return Ok(Vec::new());
    }

    let hosts: Vec<String> = raw
        .map(|s| {
            s.split(',')
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if hosts.is_empty() {
        return Err(ConfigLoadError::InvalidEnvVar {
            name: "MCP_ALLOWED_HOSTS",
            reason: "MCP_LISTEN_ADDR設定時は少なくとも1つのホストを指定する必要があります"
                .to_string(),
        });
    }

    Ok(hosts)
}

/// MCP TLS証明書/秘密鍵が両方設定されているか、両方とも未設定であることを検証する
fn validate_mcp_tls_paths(
    cert: &Option<PathBuf>,
    key: &Option<PathBuf>,
) -> Result<(), ConfigLoadError> {
    if cert.is_some() != key.is_some() {
        return Err(ConfigLoadError::InvalidEnvVar {
            name: "MCP_TLS_CERT_FILE / MCP_TLS_KEY_FILE",
            reason: "TLSを有効化するには両方を設定する必要があります".to_string(),
        });
    }
    Ok(())
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

    #[test]
    fn a_notice_policy_is_read_by_name() {
        assert_eq!(
            parse_notice_policy("IDLE_HELD_GPU_NOTICES", "observe").unwrap(),
            NoticePolicy::Observe
        );
        assert_eq!(
            parse_notice_policy("IDLE_HELD_GPU_NOTICES", " notify ").unwrap(),
            NoticePolicy::Notify
        );
    }

    #[test]
    fn an_unknown_notice_policy_is_refused_rather_than_guessed() {
        assert!(parse_notice_policy("IDLE_HELD_GPU_NOTICES", "quiet").is_err());
    }

    #[test]
    fn the_default_notice_policy_is_readable() {
        assert!(
            parse_notice_policy("IDLE_HELD_GPU_NOTICES", defaults::IDLE_HELD_GPU_NOTICES).is_ok()
        );
    }

    #[test]
    fn a_percentage_beyond_a_hundred_is_rejected() {
        assert!(validate_percentage("COMPUTING_GPU_UTILIZATION_PERCENT", 101).is_err());
        assert!(validate_percentage("COMPUTING_GPU_UTILIZATION_PERCENT", 100).is_ok());
    }

    fn dummy_addr() -> SocketAddr {
        "0.0.0.0:8787".parse().unwrap()
    }

    #[test]
    fn test_validate_mcp_allowed_hosts_skips_when_mcp_disabled() {
        let result = validate_mcp_allowed_hosts(None, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_validate_mcp_allowed_hosts_rejects_unset_when_mcp_enabled() {
        let result = validate_mcp_allowed_hosts(Some(dummy_addr()), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mcp_allowed_hosts_rejects_empty_string_when_mcp_enabled() {
        let result = validate_mcp_allowed_hosts(Some(dummy_addr()), Some("  , ,"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mcp_allowed_hosts_accepts_configured_hosts() {
        let result =
            validate_mcp_allowed_hosts(Some(dummy_addr()), Some("thalys:8787, 192.168.1.10:8787"))
                .unwrap();
        assert_eq!(result, vec!["thalys:8787", "192.168.1.10:8787"]);
    }

    #[test]
    fn test_validate_mcp_tls_paths_accepts_both_unset() {
        assert!(validate_mcp_tls_paths(&None, &None).is_ok());
    }

    #[test]
    fn test_validate_mcp_tls_paths_accepts_both_set() {
        let cert = Some(PathBuf::from("cert.pem"));
        let key = Some(PathBuf::from("key.pem"));
        assert!(validate_mcp_tls_paths(&cert, &key).is_ok());
    }

    #[test]
    fn test_validate_mcp_tls_paths_rejects_cert_without_key() {
        let cert = Some(PathBuf::from("cert.pem"));
        assert!(validate_mcp_tls_paths(&cert, &None).is_err());
    }

    #[test]
    fn test_validate_mcp_tls_paths_rejects_key_without_cert() {
        let key = Some(PathBuf::from("key.pem"));
        assert!(validate_mcp_tls_paths(&None, &key).is_err());
    }
}
