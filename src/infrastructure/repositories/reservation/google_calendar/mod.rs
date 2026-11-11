//! Google Calendar APIを使用したReservedSlotリポジトリ実装

mod event_gateway;
mod recurrence;
mod repository;
#[cfg(test)]
mod repository_tests;

pub use repository::GoogleCalendarReservationRepository;
