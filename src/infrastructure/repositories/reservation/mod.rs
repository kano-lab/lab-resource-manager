//! # Reservation Repository Implementations
//!
//! ReservedSlotQueryポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装
//! - `in_memory`: テスト用のインメモリ実装

/// Google Calendar APIを使用した予約リポジトリ実装
pub mod google_calendar;
/// テスト用のインメモリ予約リポジトリ実装
pub mod in_memory;

/// テスト用のインメモリ予約リポジトリ実装の旧名
#[deprecated(note = "in_memory::InMemoryReservationRepository を使う")]
pub mod mock {
    pub use super::in_memory::InMemoryReservationRepository as MockReservationRepository;
}
