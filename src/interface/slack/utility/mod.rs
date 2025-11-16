//! Slackインターフェースユーティリティ
//!
//! Slack統合で使用される汎用的なユーティリティ関数を提供します。
//!
//! ## 責務
//!
//! このモジュールは、Slack固有のデータ変換や解決処理などの
//! 汎用的な補助機能を提供します。
//!
//! ## モジュール
//!
//! - `extractors`: Slackペイロードからのデータ抽出
//! - `user_resolver`: SlackユーザーIDからメールアドレスへの解決

pub mod extractors;
pub mod user_resolver;
