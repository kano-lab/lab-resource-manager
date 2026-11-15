//! ポートのインメモリ実装
//!
//! 外部サービスを持たずにユースケースを動かすための実装。
//! このクレートのテストのほか、このクレートを使うアプリケーションのテストや試作でも使える。

mod idle_reserved_slot_notifier;
mod in_memory_reservation_repository;
mod reservation_proposal_notifier;
mod reserved_slot;
mod resource_usage_observer;
mod unauthorized_usage_notifier;

pub use idle_reserved_slot_notifier::MockIdleReservedSlotNotifier;
pub use in_memory_reservation_repository::InMemoryReservationRepository;
pub use reservation_proposal_notifier::MockReservationProposalNotifier;
pub use reserved_slot::reserved_slot_of_new_one_time_reservation;
pub use resource_usage_observer::MockResourceUsageObserver;
pub use unauthorized_usage_notifier::MockUnauthorizedUsageNotifier;
