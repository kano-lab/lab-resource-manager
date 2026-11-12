//! # Reservation Repository Implementations
//!
//! ReservedSlotQueryポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装
//! - `mock`: テスト用のインメモリ実装

/// Google Calendar APIを使用した予約リポジトリ実装
pub mod google_calendar;
/// テスト用のインメモリ予約リポジトリ実装
pub mod mock;
