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
//! - `extract_form_data`: Slackフォームデータの抽出
//! - `user_resolver`: SlackユーザーIDとメールアドレスの相互解決
//! - `datetime_parser`: 日付・時刻のパース
//! - `conflict_message`: リソース競合エラーからのユーザー向けメッセージ構築
//! - `reservation_summary`: 予約の対象リソースと期間の要約

pub mod conflict_message;
pub mod datetime_parser;
pub mod extract_form_data;
pub mod reservation_summary;
pub mod user_resolver;
