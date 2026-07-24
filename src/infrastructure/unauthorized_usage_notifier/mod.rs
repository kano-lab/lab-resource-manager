//! 無断使用検知の通知実装

/// テスト/開発用の記録専用実装
pub mod mock;
/// Slack DM経由の実装
pub mod slack;

pub use mock::MockUnauthorizedUsageNotifier;
pub use slack::SlackUnauthorizedUsageNotifier;
