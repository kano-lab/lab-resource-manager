//! # Slack Interface
//!
//! Slackボットインターフェースの実装を提供します。
//!
//! - `bot`: Socket ModeでSlackと接続するボット本体
//! - `commands`: スラッシュコマンドのルーティングと処理
//! - `constants`: アクションIDやコールバックID等の定数
//! - `interactions`: ボタン・モーダル等のインタラクション処理
//! - `parsers`: 入力パース処理
//! - `views`: UI/View構築

/// Slack Botの本体実装
pub mod bot;
/// Slackコマンドハンドラ
pub mod commands;
/// 定数定義
pub mod constants;
/// インタラクションハンドラ
pub mod interactions;
/// 入力パーサー
pub mod parsers;
/// View/UI構築
pub mod views;

pub use bot::SlackBot;
pub use commands::SlackCommandHandler;
