//! # Notification Senders
//!
//! 各種通知サービスへの送信を担当する実装を提供します。
//!
//! - `sender`: 送信手段の共通トレイト定義
//! - `slack`: Slack Bot Token経由の通知送信
//! - `mock`: テスト/開発用のモック送信実装

/// モック通知送信実装
pub mod mock;
/// 通知送信の共通トレイト
pub mod sender;
/// Slack通知送信実装
pub mod slack;

pub use mock::MockSender;
pub use sender::Sender;
pub use slack::SlackSender;
