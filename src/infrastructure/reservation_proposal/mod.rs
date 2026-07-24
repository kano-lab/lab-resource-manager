//! 未予約利用の事後予約提案の実装

/// テスト/開発用の記録専用実装
pub mod mock;
/// Slack DM経由の実装
pub mod slack;

pub use mock::MockReservationProposalNotifier;
pub use slack::{ProposalAcceptPayload, SlackReservationProposalNotifier};
