//! # ResourceCollectionAccess Service Implementations
//!
//! ResourceCollectionAccessServiceポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装

/// Google Calendar APIを使用したリソースコレクションアクセスサービス実装
pub mod google_calendar;

pub use google_calendar::GoogleCalendarAccessService;
