//! # Notification Senders
//!
//! 各種通知サービスへの送信を担当する実装を提供します。
//!
//! - `sender`: 送信手段の共通トレイト定義
//! - `slack`: Slack Webhook経由の通知送信
//! - `mock`: テスト/開発用のモック送信実装
pub mod mock;
pub mod sender;
pub mod slack;

pub use mock::MockSender;
pub use sender::Sender;
pub use slack::SlackSender;
