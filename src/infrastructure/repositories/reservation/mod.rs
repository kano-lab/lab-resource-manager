//! # Reservation Repository Implementations
//!
//! ReservedSlotQueryポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装
//!
//! テスト用のインメモリ実装は`lab_resource_manager_core::testing`にある。

#[cfg(test)]
mod contract_tests;
/// Google Calendar APIを使用した予約リポジトリ実装
pub mod google_calendar;
/// テスト用のインメモリ予約リポジトリ実装の旧名
#[deprecated(note = "lab_resource_manager_core::testing::InMemoryReservationRepository を使う")]
pub mod mock {
    pub use crate::testing::InMemoryReservationRepository as MockReservationRepository;
}
