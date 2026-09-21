//! Google Calendar APIを使用したResourceUsageリポジトリ実装

mod event_gateway;
mod repository;
#[cfg(test)]
mod repository_tests;

pub use repository::GoogleCalendarUsageRepository;
