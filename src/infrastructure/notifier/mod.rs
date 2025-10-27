//! # Notifier Implementations
//!
//! Notifierポートの具象実装を提供します。
//!
//! - `router`: リソース設定に基づいて複数の通知手段をオーケストレート
//! - `senders`: 個別の送信手段の実装（Slack, Mock, Discord, Email等）
pub mod router;
pub mod senders;
