//! 予約の要約（対象リソースと期間）の構築
//!
//! 予約チャンネルへの通知と、操作へのフィードバックで時刻やリソースの書き方が変わると、
//! 同じ予約の話をしていることが読み手に伝わりにくくなる。対象リソースに設定された
//! 通知設定（タイムゾーン・フォーマットスタイル）を流用して見え方を揃える。

use crate::domain::aggregates::reservation::ReservedSlot;
use crate::infrastructure::config::ResourceConfig;
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};

/// 予約の対象リソースと期間を2行にまとめる
///
/// 複数リソースをまとめた予約では先頭のリソースの通知設定を使う。
/// 1件の予約が複数の設定にまたがることは通常なく、見え方を1つに決める必要があるため。
pub fn build(slot: &ReservedSlot, resource_config: &ResourceConfig) -> String {
    let notification = slot
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
        format_resources_styled(slot.resources(), customization.format.resource_style),
        format_time_styled(
            slot.time_period(),
            timezone.as_deref(),
            customization.format.time_style,
            customization.format.date_format,
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::common::EmailAddress;
    use crate::domain::common::TimePeriod;
    use crate::infrastructure::config::{DeviceConfig, ServerConfig};
    use chrono::{TimeZone, Utc};

    fn config() -> ResourceConfig {
        ResourceConfig {
            servers: vec![ServerConfig {
                name: "Thalys".to_string(),
                devices: vec![DeviceConfig {
                    id: 0,
                    model: "A100".to_string(),
                }],
                notifications: vec![],
            }],
            rooms: vec![],
        }
    }

    fn slot() -> ReservedSlot {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        ReservedSlot::of_new_one_time_reservation(
            EmailAddress::new("holder@example.com".to_string()).unwrap(),
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
        let summary = build(&slot(), &config());

        assert!(
            summary.contains("Thalys"),
            "対象リソースが分かる: {summary}"
        );
        assert_eq!(summary.lines().count(), 2, "1行に詰めない: {summary}");
    }
}
