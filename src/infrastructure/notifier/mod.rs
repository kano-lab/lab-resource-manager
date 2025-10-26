//! # Notifier Implementations
//!
//! Notifierポートの具象実装を提供します。
//!
//! - `slack`: Slack Webhook経由での通知
//! - `mock`: テスト用のモック実装（標準出力）
pub mod mock;
pub mod slack;
