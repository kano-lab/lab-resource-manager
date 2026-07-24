//! 未予約利用の事後予約提案の実装
//!
//! 実際のSlack DM送信実装は未着手のため、現時点ではテスト・開発用のMock実装のみを提供する。

/// テスト/開発用の記録専用実装
pub mod mock;

pub use mock::MockReservationProposalNotifier;
