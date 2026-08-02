//! 使われていない予約の通知実装

mod mock;
mod slack;

pub use mock::MockIdleReservationNotifier;
pub use slack::SlackIdleReservationNotifier;
