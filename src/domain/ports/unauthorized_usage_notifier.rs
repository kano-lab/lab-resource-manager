use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;

/// 予約と異なる利用者によるリソース利用（無断使用）を検知した際に、
/// 実際に利用していた本人へ直接通知するポート
///
/// 予約チャンネルへのブロードキャスト通知（`Notifier`）とは異なり、
/// 特定の利用者本人への直接的な通知（Slack DM等）を前提とするため別ポートとする。
#[async_trait]
pub trait UnauthorizedUsageNotifier: Send + Sync {
    /// 無断使用を行った本人へ通知する
    async fn notify(
        &self,
        reserved_usage: &ResourceUsage,
        actual_user_email: &EmailAddress,
    ) -> Result<(), NotificationError>;
}
