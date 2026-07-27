use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::reservation_proposal::{
    ReservationProposal, ReservationProposalNotifier,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// テスト/開発用の記録専用実装
///
/// 送信内容を標準出力に表示しつつ、テストからの検証用に内部へ保持する。
#[derive(Clone, Default)]
pub struct MockReservationProposalNotifier {
    proposals: Arc<Mutex<Vec<ReservationProposal>>>,
}

impl MockReservationProposalNotifier {
    /// 新しいモック実装を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// これまでに送信された提案の一覧を取得
    pub fn sent_proposals(&self) -> Vec<ReservationProposal> {
        self.proposals.lock().unwrap().clone()
    }
}

#[async_trait]
impl ReservationProposalNotifier for MockReservationProposalNotifier {
    async fn propose(&self, proposal: ReservationProposal) -> Result<(), NotificationError> {
        println!(
            "📤 [MockReservationProposalNotifier] {} へ {} の予約を提案 (候補: {}件)",
            proposal.owner_email().as_str(),
            proposal
                .resources()
                .iter()
                .map(|resource| resource.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            proposal.duration_candidates().len()
        );
        self.proposals.lock().unwrap().push(proposal);
        Ok(())
    }
}
