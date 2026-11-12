//! ドメインサービス
//!
//! エンティティや値オブジェクトに属さない、ドメインロジックを提供するサービス群。
//!
//! # モジュール
//!
//! - `authorization` - リソース操作の認可を管理
//! - `reservation` - 予約枠に関するビジネスロジック

pub mod authorization;
pub mod reservation;

pub use authorization::{AuthorizationError, AuthorizationPolicy, ReservationAuthorizationPolicy};
pub use reservation::ResourceConflictChecker;
