//! 予約の要約（対象リソースと期間）の構築
//!
//! 予約チャンネルへの通知と、操作へのフィードバックで時刻やリソースの書き方が変わると、
//! 同じ予約の話をしていることが読み手に伝わりにくくなる。対象リソースに設定された
//! 通知設定（タイムゾーン・フォーマットスタイル）を流用して見え方を揃える。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::infrastructure::config::ResourceConfig;
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};

/// 予約の対象リソースと期間を2行にまとめる
///
/// 複数リソースをまとめた予約では先頭のリソースの通知設定を使う。
/// 1件の予約が複数の設定にまたがることは通常なく、見え方を1つに決める必要があるため。
pub fn build(usage: &ResourceUsage, resource_config: &ResourceConfig) -> String {
    let notification = usage
        .resources()
        .first()
        .map(|resource| resource_config.get_notifications_for_resource(resource))
        .unwrap_or_default()
        .into_iter()
        .next();

    let customization = notification
        .as_ref()
        .map(|config| config.customization())
        .unwrap_or_default();
    let timezone = notification
        .as_ref()
        .and_then(|config| config.timezone())
        .map(str::to_string);

    format!(
        "💻 {}\n📅 {}",
        format_resources_styled(usage.resources(), customization.format.resource_style),
        format_time_styled(
            usage.time_period(),
            timezone.as_deref(),
            customization.format.time_style,
            customization.format.date_format,
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::config::{DeviceConfig, ServerConfig};
    use chrono::{TimeZone, Utc};

    fn config() -> ResourceConfig {
        ResourceConfig {
            servers: vec![ServerConfig {
                name: "Thalys".to_string(),
                calendar_id: "cal".to_string(),
                devices: vec![DeviceConfig {
                    id: 0,
                    model: "A100".to_string(),
                }],
                notifications: vec![],
            }],
            rooms: vec![],
        }
    }

    fn usage() -> ResourceUsage {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[test]
    fn the_summary_names_the_resource_and_the_period() {
        let summary = build(&usage(), &config());

        assert!(
            summary.contains("Thalys"),
            "対象リソースが分かる: {summary}"
        );
        assert_eq!(summary.lines().count(), 2, "1行に詰めない: {summary}");
    }
}
