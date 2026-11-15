//! # Notifier Implementations
//!
//! Notifierポートの具象実装を提供します。
//!
//! - `router`: リソース設定に基づいて複数の通知手段をオーケストレート
//! - `senders`: 個別の送信手段の実装（Slack, Mock, Discord, Email等）
//! - `formatter`: スタイル別フォーマット関数
//! - `template_renderer`: テンプレートレンダリング

/// スタイル別フォーマット関数
pub mod formatter;
/// 通知ルーター実装
pub mod router;
/// 通知送信実装
pub mod senders;
/// テンプレートレンダリング
pub mod template_renderer;

pub use router::NotificationRouter;
