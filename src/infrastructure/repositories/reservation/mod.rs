//! # ReservedSlot Repository Implementations
//!
//! ReservationRepositoryポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装
//! - `mock`: テスト用のインメモリ実装

/// Google Calendar APIを使用したReservedSlotリポジトリ実装
pub mod google_calendar;
/// テスト用のモックReservedSlotリポジトリ実装
pub mod mock;
