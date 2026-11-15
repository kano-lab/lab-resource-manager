//! 監視の稼働状況メッセージ
//!
//! 運用者が「いま監視は効いているか」を一目で判断できる形にする。
//! 効いていないサーバーについては、次に何を見ればよいかが分かるように理由を添える。

use crate::application::usecases::describe_monitoring::{MonitoringSettings, MonitoringStatus};
use crate::domain::ports::ServerObservation;
use chrono::{DateTime, Duration, Utc};

/// 稼働状況をメッセージ本文に整形する
///
/// # Arguments
/// * `status` - 監視の稼働状況
/// * `version` - 動いているlab-resource-managerのバージョン
/// * `started_at` - このプロセスが動き始めた時刻
/// * `now` - 現在時刻（経過時間の基準）
pub fn build(
    status: &MonitoringStatus,
    version: &str,
    started_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    let header = format!(
        "🤖 lab-resource-manager {} ・ 起動から{}",
        version,
        format_elapsed(now - started_at)
    );

    match status {
        MonitoringStatus::Disabled => format!(
            "{}\n\n🔍 実利用の監視: 無効\n\nGPU_USAGE_REPORTS_DIR が設定されていないため、実際の利用は観測していません。未予約利用の提案も、使われていない予約のお知らせも行いません。",
            header
        ),
        MonitoringStatus::Enabled { servers, settings } => {
            format!(
                "{}\n\n{}\n\n{}",
                header,
                build_servers(servers, now),
                build_settings(settings)
            )
        }
    }
}

/// サーバーごとの状態を1行ずつ並べる
///
/// 印を行頭に置くことで、手当てが要るサーバーだけを目で拾える。
fn build_servers(servers: &[(String, ServerObservation)], now: DateTime<Utc>) -> String {
    if servers.is_empty() {
        return "🔍 実利用の監視: 対象のGPUサーバーが設定されていません".to_string();
    }

    let lines: Vec<String> = servers
        .iter()
        .map(|(name, observation)| {
            let (mark, detail) = describe_observation(observation, now);
            format!("{} `{}` {}", mark, name, detail)
        })
        .collect();

    format!("🔍 実利用の監視\n{}", lines.join("\n"))
}

/// 1サーバーの状態を、印と、次に何を見ればよいかが分かる言葉にする
fn describe_observation(
    observation: &ServerObservation,
    now: DateTime<Utc>,
) -> (&'static str, String) {
    match observation {
        ServerObservation::Observed { generated_at } => (
            "✅",
            format!("{}のレポート", format_ago(now - *generated_at)),
        ),
        ServerObservation::Stale { generated_at } => (
            "⚠️",
            format!(
                "{}のレポートで止まっています",
                format_ago(now - *generated_at)
            ),
        ),
        ServerObservation::Missing => (
            "❌",
            "レポートが届いていません（gpu-usage-reporter の実行を確認してください）".to_string(),
        ),
        ServerObservation::Unreadable => (
            "❌",
            "レポートを読めません（ファイルの中身と権限を確認してください）".to_string(),
        ),
    }
}

/// 状態を読むための尺度を添える
fn build_settings(settings: &MonitoringSettings) -> String {
    format!(
        "突合は{}ごと。{}より古いレポートは使いません。{}使われていない予約と、{}計算が走らないGPUは予約者に知らせます。",
        format_elapsed(settings.polling_interval),
        format_elapsed(settings.max_staleness),
        format_elapsed(settings.idle_threshold),
        format_elapsed(settings.held_gpu_threshold)
    )
}

/// 経過時間を読みやすい長さに丸める（例: "3日4時間", "42分"）
///
/// 分より短い単位は運用の判断に効かないため、切り捨てて「1分未満」とまとめる。
fn format_elapsed(elapsed: Duration) -> String {
    let minutes = elapsed.num_minutes();
    if minutes < 1 {
        return "1分未満".to_string();
    }

    let days = minutes / (60 * 24);
    let hours = (minutes % (60 * 24)) / 60;
    let remaining_minutes = minutes % 60;

    match (days, hours, remaining_minutes) {
        (0, 0, m) => format!("{}分", m),
        (0, h, 0) => format!("{}時間", h),
        (0, h, m) => format!("{}時間{}分", h, m),
        (d, 0, _) => format!("{}日", d),
        (d, h, _) => format!("{}日{}時間", d, h),
    }
}

/// 過去の時刻までの隔たり（例: "1分前", "42分前"）
fn format_ago(elapsed: Duration) -> String {
    format!("{}前", format_elapsed(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "1.7.0";

    fn settings() -> MonitoringSettings {
        MonitoringSettings {
            polling_interval: Duration::seconds(60),
            idle_threshold: Duration::minutes(30),
            held_gpu_threshold: Duration::hours(1),
            max_staleness: Duration::minutes(5),
        }
    }

    fn enabled(servers: Vec<(String, ServerObservation)>) -> MonitoringStatus {
        MonitoringStatus::Enabled {
            servers,
            settings: settings(),
        }
    }

    #[test]
    fn a_disabled_monitor_says_what_is_not_happening() {
        let now = Utc::now();

        let message = build(&MonitoringStatus::Disabled, VERSION, now, now);

        assert!(message.contains("無効"), "{message}");
        assert!(
            message.contains("GPU_USAGE_REPORTS_DIR"),
            "何を設定すれば有効になるのかが分かるべき: {message}"
        );
    }

    #[test]
    fn every_server_gets_a_line_with_its_state() {
        let now = Utc::now();
        let status = enabled(vec![
            (
                "Freccia".to_string(),
                ServerObservation::Observed {
                    generated_at: now - Duration::minutes(1),
                },
            ),
            (
                "Thalys".to_string(),
                ServerObservation::Stale {
                    generated_at: now - Duration::minutes(42),
                },
            ),
            ("Alfa".to_string(), ServerObservation::Missing),
        ]);

        let message = build(&status, VERSION, now - Duration::hours(3), now);

        assert!(message.contains("1分前のレポート"), "{message}");
        assert!(
            message.contains("42分前のレポートで止まっています"),
            "{message}"
        );
        assert!(message.contains("レポートが届いていません"), "{message}");
    }

    #[test]
    fn an_unreachable_server_points_at_what_to_check() {
        let now = Utc::now();

        for (observation, expected) in [
            (ServerObservation::Missing, "gpu-usage-reporter"),
            (ServerObservation::Unreadable, "権限"),
        ] {
            let message = build(
                &enabled(vec![("Thalys".to_string(), observation)]),
                VERSION,
                now,
                now,
            );
            assert!(
                message.contains(expected),
                "手当ての手がかりがない ({expected}): {message}"
            );
        }
    }

    #[test]
    fn the_running_version_and_uptime_are_stated() {
        let now = Utc::now();

        let message = build(
            &MonitoringStatus::Disabled,
            VERSION,
            now - Duration::days(3) - Duration::hours(4),
            now,
        );

        assert!(message.contains(VERSION), "{message}");
        assert!(message.contains("3日4時間"), "{message}");
    }

    #[test]
    fn elapsed_time_is_rounded_to_units_that_matter() {
        assert_eq!(format_elapsed(Duration::seconds(30)), "1分未満");
        assert_eq!(format_elapsed(Duration::minutes(42)), "42分");
        assert_eq!(format_elapsed(Duration::hours(2)), "2時間");
        assert_eq!(format_elapsed(Duration::minutes(150)), "2時間30分");
        assert_eq!(format_elapsed(Duration::days(3)), "3日");
        assert_eq!(
            format_elapsed(Duration::days(3) + Duration::hours(4) + Duration::minutes(9)),
            "3日4時間",
            "日単位まで来たら分は判断に効かない"
        );
    }

    #[test]
    fn a_monitor_without_any_server_is_not_mistaken_for_a_healthy_one() {
        let now = Utc::now();

        let message = build(&enabled(vec![]), VERSION, now, now);

        assert!(message.contains("設定されていません"), "{message}");
    }
}
