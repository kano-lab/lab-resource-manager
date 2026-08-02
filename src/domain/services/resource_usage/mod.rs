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
//! - `availability` - 対象期間における各リソースの空きを算出
//! - `conflict_checker` - リソースの時間的競合をチェック
//! - `errors` - サービス層のエラー型定義
//! - `reservation_activity` - 予約が予約者本人に使われているかの見立て

pub mod availability;
pub mod conflict_checker;
pub mod errors;
pub mod reservation_activity;

pub use availability::{AvailabilityState, BusyPeriod, ResourceAvailability};
pub use conflict_checker::ResourceConflictChecker;
pub use errors::ResourceConflictError;
pub use reservation_activity::{GpusAtRest, ReservationActivity, judge_reservation_activity};
