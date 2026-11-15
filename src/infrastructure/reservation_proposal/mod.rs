//! 未予約利用の事後予約提案の実装

/// Slack DM経由の実装
pub mod slack;

pub use slack::{ProposalAcceptPayload, SlackReservationProposalNotifier};
