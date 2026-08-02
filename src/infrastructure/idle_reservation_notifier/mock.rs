use crate::domain::ports::idle_reservation_notifier::{IdleReservation, IdleReservationNotifier};
use crate::domain::ports::notifier::NotificationError;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// テスト/開発用の記録専用実装
///
/// 送信内容を標準出力に表示しつつ、テストからの検証用に内部へ保持する。
#[derive(Clone, Default)]
pub struct MockIdleReservationNotifier {
    notices: Arc<Mutex<Vec<IdleReservation>>>,
    fails: Arc<Mutex<bool>>,
}

impl MockIdleReservationNotifier {
    /// 新しいモック実装を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// これまでに送信された通知の一覧を取得
    pub fn sent_notices(&self) -> Vec<IdleReservation> {
        self.notices.lock().unwrap().clone()
    }

    /// 以降の送信を失敗させるかどうかを切り替える
    pub fn set_failing(&self, failing: bool) {
        *self.fails.lock().unwrap() = failing;
    }
}

#[async_trait]
impl IdleReservationNotifier for MockIdleReservationNotifier {
    async fn notify_idle(&self, idle: IdleReservation) -> Result<(), NotificationError> {
        if *self.fails.lock().unwrap() {
            return Err(NotificationError::SendFailure(
                "test induced failure".to_string(),
            ));
        }

        println!(
            "📤 [MockIdleReservationNotifier] {} の予約 {} が {} から使われていない",
            idle.reservation().owner_email().as_str(),
            idle.reservation().id().as_str(),
            idle.idle_since()
        );
        self.notices.lock().unwrap().push(idle);
        Ok(())
    }
}
