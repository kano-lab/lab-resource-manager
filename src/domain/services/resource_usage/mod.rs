//! リソース使用予定に関するドメインサービス
//!
//! リソース使用予定（ResourceUsage）に関連するビジネスロジックを提供する。
//!
//! # 概要
//!
//! このモジュールは、複数のResourceUsageエンティティにまたがる操作や、
//! 外部リポジトリとの連携が必要なドメインロジックを実装します。
//!
//! # モジュール
//!
//! - `conflict_checker` - リソースの時間的競合をチェック
//! - `errors` - サービス層のエラー型定義

pub mod conflict_checker;
pub mod errors;

pub use conflict_checker::ResourceConflictChecker;
pub use errors::ResourceConflictError;
