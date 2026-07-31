//! Slack DM経由の無断使用検知通知実装

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::unauthorized_usage_notifier::UnauthorizedUsageNotifier;
use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};
use crate::infrastructure::slack_direct_message::SlackDirectMessenger;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slack DM経由で無断使用検知を本人へ通知する実装
pub struct SlackUnauthorizedUsageNotifier {
    messenger: SlackDirectMessenger,
}

impl SlackUnauthorizedUsageNotifier {
    /// 新しい実装を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            messenger: SlackDirectMessenger::new(slack_client, bot_token, identity_repo),
        }
    }
}

#[async_trait]
impl UnauthorizedUsageNotifier for SlackUnauthorizedUsageNotifier {
    async fn notify(
        &self,
        reserved_usage: &ResourceUsage,
        actual_user_email: &EmailAddress,
    ) -> Result<(), NotificationError> {
        let text = build_unauthorized_message(reserved_usage);

        let sent = self
            .messenger
            .send(actual_user_email, text, Vec::new())
            .await?;

        tracing::info!(
            channel = %sent.channel,
            ts = %sent.ts,
            usage_id = %reserved_usage.id().as_str(),
            recipient = %actual_user_email.as_str(),
            "sent an unauthorized-usage dm"
        );

        Ok(())
    }
}

/// 無断使用検知のDMメッセージを構築する（純粋関数、ユニットテスト対象）
fn build_unauthorized_message(reserved_usage: &ResourceUsage) -> String {
    let resources = format_resources_styled(reserved_usage.resources(), ResourceStyle::Full);
    let time = format_time_styled(
        reserved_usage.time_period(),
        None,
        TimeStyle::Full,
        DateFormat::Ymd,
    );

    format!(
        "🚫 他の人が予約中のリソースを使用しています\n\nあなたが使用中のリソースには、現在以下の予約が入っています。\n\n👤 予約者\n{}\n\n📅 予約期間\n{}\n\n💻 予約リソース\n{}\n\n心当たりがある場合は、処理を停止するか予約者と調整してください。",
        reserved_usage.owner_email().as_str(),
        time,
        resources
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use chrono::{TimeZone, Utc};

    fn sample_usage() -> ResourceUsage {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = TimePeriod::new(start, end).unwrap();
        ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
            period,
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
    fn test_build_unauthorized_message_contains_owner_and_resource() {
        let usage = sample_usage();
        let message = build_unauthorized_message(&usage);

        assert!(message.contains("owner@example.com"));
        assert!(message.contains("Thalys"));
        assert!(message.contains("他の人が予約中のリソースを使用"));
    }
}
