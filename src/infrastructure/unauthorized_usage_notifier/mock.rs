use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::unauthorized_usage_notifier::UnauthorizedUsageNotifier;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// テスト/開発用の記録専用実装
///
/// 送信内容を標準出力に表示しつつ、テストからの検証用に内部へ保持する。
#[derive(Clone, Default)]
pub struct MockUnauthorizedUsageNotifier {
    notified: Arc<Mutex<Vec<(ResourceUsage, EmailAddress)>>>,
}

impl MockUnauthorizedUsageNotifier {
    /// 新しいモック実装を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// これまでに送信された通知の一覧を取得
    pub fn notified(&self) -> Vec<(ResourceUsage, EmailAddress)> {
        self.notified.lock().unwrap().clone()
    }
}

#[async_trait]
impl UnauthorizedUsageNotifier for MockUnauthorizedUsageNotifier {
    async fn notify(
        &self,
        reserved_usage: &ResourceUsage,
        actual_user_email: &EmailAddress,
    ) -> Result<(), NotificationError> {
        println!(
            "📤 [MockUnauthorizedUsageNotifier] {} へ {} の無断使用を通知",
            actual_user_email.as_str(),
            reserved_usage.owner_email().as_str()
        );
        self.notified
            .lock()
            .unwrap()
            .push((reserved_usage.clone(), actual_user_email.clone()));
        Ok(())
    }
}
