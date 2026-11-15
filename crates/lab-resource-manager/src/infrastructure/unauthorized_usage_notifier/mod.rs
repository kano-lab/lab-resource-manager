//! 無断使用検知の通知実装

/// Slack DM経由の実装
pub mod slack;

pub use slack::SlackUnauthorizedUsageNotifier;
