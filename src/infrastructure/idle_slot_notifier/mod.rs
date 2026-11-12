//! 使われていない予約の通知実装

mod mock;
mod slack;

pub use mock::MockIdleSlotNotifier;
pub use slack::SlackIdleSlotNotifier;
