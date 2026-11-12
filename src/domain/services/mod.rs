//! ドメインサービス
//!
//! エンティティや値オブジェクトに属さない、ドメインロジックを提供するサービス群。
//!
//! # モジュール
//!
//! - `reservation` - 予約と予約枠に関するビジネスロジック
//!
//! 「予約者本人だけが変えられる」という決まりは、予約枠の操作そのもの
//! （`ReservedSlot::reschedule`など）が確かめる。操作と別の認可サービスは置かない。

pub mod reservation;

pub use reservation::{ReservationChangeInterpreter, ResourceConflictChecker};
