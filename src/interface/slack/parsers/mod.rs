//! Slack入力データパーサー
//!
//! Slackインタラクションからの入力データをドメイン層の型に変換します。
//!
//! ## 責務
//!
//! Slackのフォームから取得した文字列データ（日付、時刻、リソースIDなど）を、
//! ドメイン層で使用できる型（DateTime、u32など）にパースします。
//!
//! このモジュールはSlack特有の入力形式（"2024-01-15"、"14:30"、"GPU #0"など）を
//! 理解し、適切な型に変換する責務を持ちます。
//!
//! ## モジュール
//!
//! - `datetime`: 日付・時刻文字列のパース（"YYYY-MM-DD" + "HH:MM" → `DateTime<Local>`）
//! - `resource`: リソースID文字列のパース（"GPU #0" → `0`）

pub mod datetime;
pub mod resource;

pub use datetime::parse_datetime;
pub use resource::parse_device_id;
