//! # Slack Interface
//!
//! Slackボットインターフェースの実装を提供します。
//!
//! - `bot`: Socket ModeでSlackと接続するボット本体
//! - `commands`: スラッシュコマンドのルーティングと処理

/// Slack Botの本体実装
pub mod bot;
/// Slackコマンドハンドラ
pub mod commands;

pub use bot::SlackBot;
pub use commands::SlackCommandHandler;
