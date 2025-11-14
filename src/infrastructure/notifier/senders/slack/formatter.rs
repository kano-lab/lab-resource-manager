//! Slackメッセージフォーマット機能

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::service::{format_resources, format_time_period};
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::notifier::NotificationEvent;
use crate::infrastructure::notifier::senders::sender::NotificationContext;

/// Slackメッセージフォーマッター
pub struct SlackMessageFormatter;

impl SlackMessageFormatter {
    /// リソースタイプに応じたラベルを生成
    pub fn get_resource_label(resources: &[Resource]) -> &'static str {
        if resources.is_empty() {
            return "📦 予約リソース";
        }

        let has_gpu = resources.iter().any(|r| matches!(r, Resource::Gpu(_)));
        let has_room = resources.iter().any(|r| matches!(r, Resource::Room { .. }));

        match (has_gpu, has_room) {
            (true, false) => "💻 予約GPU",
            (false, true) => "🏢 予約部屋",
            _ => "📦 予約リソース", // 混在または不明
        }
    }

    /// ユーザー表示名をフォーマット（Slackメンション or メールアドレス）
    pub fn format_user(
        email: &crate::domain::common::EmailAddress,
        identity_link: Option<&crate::domain::aggregates::identity_link::entity::IdentityLink>,
    ) -> String {
        if let Some(identity) = identity_link
            && let Some(slack_identity) = identity.get_identity_for_system(&ExternalSystem::Slack)
        {
            return format!("<@{}>", slack_identity.user_id());
        }
        email.as_str().to_string()
    }

    /// イベントからSlack用のメッセージを構築
    pub fn format_message(context: &NotificationContext) -> String {
        let usage = Self::extract_usage_from_event(context.event);
        let user_display = Self::format_user(usage.owner_email(), context.identity_link);
        let resources = format_resources(usage.resources());
        let time_period = format_time_period(usage.time_period(), context.timezone);
        let resource_label = Self::get_resource_label(usage.resources());

        match context.event {
            NotificationEvent::ResourceUsageCreated(_) => {
                format!(
                    "🔔 新規予約\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
            }
            NotificationEvent::ResourceUsageUpdated(_) => {
                format!(
                    "🔄 予約更新\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
            }
            NotificationEvent::ResourceUsageDeleted(_) => {
                format!(
                    "🗑️ 予約削除\n👤 {}\n\n📅 期間\n{}\n\n{}\n{}",
                    user_display, time_period, resource_label, resources
                )
            }
        }
    }

    /// イベントからResourceUsageを抽出
    fn extract_usage_from_event(event: &NotificationEvent) -> &ResourceUsage {
        match event {
            NotificationEvent::ResourceUsageCreated(u) => u,
            NotificationEvent::ResourceUsageUpdated(u) => u,
            NotificationEvent::ResourceUsageDeleted(u) => u,
        }
    }
}
