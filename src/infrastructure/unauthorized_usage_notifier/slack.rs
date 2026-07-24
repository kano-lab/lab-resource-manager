//! Slack DM経由の無断使用検知通知実装

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::unauthorized_usage_notifier::UnauthorizedUsageNotifier;
use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};
use async_trait::async_trait;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slack DM経由で無断使用検知を本人へ通知する実装
pub struct SlackUnauthorizedUsageNotifier {
    slack_client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,
    identity_repo: Arc<dyn IdentityLinkRepository>,
}

impl SlackUnauthorizedUsageNotifier {
    /// 新しい実装を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            slack_client,
            bot_token,
            identity_repo,
        }
    }

    /// 通知先（実際の利用者）のSlackユーザーIDを解決する
    async fn resolve_slack_user_id(
        &self,
        actual_user_email: &EmailAddress,
    ) -> Result<SlackUserId, NotificationError> {
        let identity_link = self
            .identity_repo
            .find_by_email(actual_user_email)
            .await
            .map_err(|e| {
                NotificationError::RepositoryError(format!("IdentityLink取得失敗: {}", e))
            })?
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "IdentityLink未登録のためDMを送信できません: {}",
                    actual_user_email.as_str()
                ))
            })?;

        let slack_identity = identity_link
            .get_identity_for_system(&ExternalSystem::Slack)
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "Slackアカウント未リンクのためDMを送信できません: {}",
                    actual_user_email.as_str()
                ))
            })?;

        Ok(SlackUserId::new(slack_identity.user_id().to_string()))
    }
}

#[async_trait]
impl UnauthorizedUsageNotifier for SlackUnauthorizedUsageNotifier {
    async fn notify(
        &self,
        reserved_usage: &ResourceUsage,
        actual_user_email: &EmailAddress,
    ) -> Result<(), NotificationError> {
        let slack_user_id = self.resolve_slack_user_id(actual_user_email).await?;

        let session = self.slack_client.open_session(&self.bot_token);

        let open_resp = session
            .conversations_open(
                &SlackApiConversationsOpenRequest::new().with_users(vec![slack_user_id]),
            )
            .await
            .map_err(|e| {
                NotificationError::SendFailure(format!("DMチャンネルのオープンに失敗: {}", e))
            })?;

        let text = build_unauthorized_message(reserved_usage);

        let post_req = SlackApiChatPostMessageRequest::new(
            open_resp.channel.id,
            SlackMessageContent::new().with_text(text),
        );

        session
            .chat_post_message(&post_req)
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack DM送信失敗: {}", e)))?;

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
        "⚠️ 予約なしでリソースを使用しています\n\n📅 予約期間\n{}\n\n💻 予約リソース\n{}\n\n予約者: {}\n\n予約せずに利用している場合は、事前に予約を行ってください。",
        time,
        resources,
        reserved_usage.owner_email().as_str()
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
        assert!(message.contains("予約なしでリソースを使用"));
    }
}
