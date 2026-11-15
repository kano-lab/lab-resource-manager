//! Google Calendar APIを使用した予約リポジトリ実装

#[cfg(test)]
mod contract_tests;
mod event_gateway;
#[cfg(test)]
mod fetch_concurrency_tests;
mod repository;
#[cfg(test)]
pub(super) mod repository_tests;

pub use repository::GoogleCalendarReservationRepository;
