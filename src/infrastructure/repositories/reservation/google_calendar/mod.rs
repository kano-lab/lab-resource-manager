//! Google Calendar APIを使用した予約リポジトリ実装

mod event_gateway;
mod repository;
#[cfg(test)]
mod repository_tests;

pub use repository::GoogleCalendarReservationRepository;
