//! Slack APIクライアント
//!
//! Slack APIへのラッパー関数を提供します。
//!
//! ## 責務
//!
//! Slack公式APIの呼び出しをカプセル化し、アプリケーション全体で
//! 一貫したインターフェースでSlack APIを使用できるようにします。
//!
//! このモジュールは、slack-morphismクレートのAPI呼び出しをラップし、
//! エラーハンドリングやログ出力を統一的に行います。
//!
//! ## Slack API との対応
//!
//! | Slack API | このモジュール |
//! |-----------|---------------|
//! | `chat.postMessage` | `messages::send_followup()` |
//! | `chat.postEphemeral` | `messages::send_ephemeral()` |
//!
//! ## モジュール
//!
//! - `messages`: メッセージ送信（通常メッセージ、エフェメラルメッセージ）

pub mod messages;
