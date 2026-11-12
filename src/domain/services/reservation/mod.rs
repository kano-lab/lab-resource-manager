//! 予約と予約枠に関するドメインサービス
//!
//! 予約（Reservation）と予約枠（ReservedSlotView）に関連するビジネスロジックを提供する。
//!
//! # 概要
//!
//! このモジュールは、複数のReservedSlotViewエンティティにまたがる操作や、
//! 外部リポジトリとの連携が必要なドメインロジックを実装します。
//!
//! # モジュール
//!
//! - `availability` - 対象期間における各リソースの空きを算出
//! - `change_interpreter` - 予約枠の見え方の変化から、予約に起きた出来事を読み取る
//! - `conflict_checker` - リソースの時間的競合をチェック
//! - `errors` - サービス層のエラー型定義
//! - `slot_activity` - 予約枠が使われているかの見立て

pub mod availability;
pub mod change_interpreter;
pub mod conflict_checker;
pub mod errors;
pub mod reserved_slot_activity;

pub use availability::{AvailabilityState, BusyPeriod, ResourceAvailability};
pub use change_interpreter::{InterpretedChanges, ReservationChangeInterpreter};
pub use conflict_checker::ResourceConflictChecker;
pub use errors::ResourceConflictError;
pub use reserved_slot_activity::{GpusAtRest, ReservedSlotActivity, judge_reserved_slot_activity};
