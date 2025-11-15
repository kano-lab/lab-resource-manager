//! ドメインサービス
//!
//! エンティティや値オブジェクトに属さない、ドメインロジックを提供するサービス群。
//!
//! # モジュール
//!
//! - `authorization` - リソース操作の認可を管理
//! - `resource_usage` - リソース使用予定に関するビジネスロジック

pub mod authorization;
pub mod resource_usage;

pub use authorization::{
    AuthorizationError, AuthorizationPolicy, ResourceUsageAuthorizationPolicy,
};
pub use resource_usage::ResourceConflictChecker;
