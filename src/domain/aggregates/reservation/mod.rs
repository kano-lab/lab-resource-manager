//! # Reservation集約
//!
//! 研究室のリソース（GPU、部屋）の予約を扱う集約です。
//!
//! ## 言葉
//!
//! - **予約**（[`Reservation`]）: 予約者が、リソースを使う時間を前もって押さえる約束。
//!   作る・取り消すの単位である
//! - **単発予約**（[`OneTimeReservation`]）: 期間が1つに決まっている予約
//! - **予約枠**（[`ReservedSlot`]）: 予約が実際に押さえている、ひとつながりの時間。
//!   単発予約は予約枠を1つ持つ
//!
//! 競合の判定、空きの計算、使われていない予約枠の検知、今で終了は、予約枠に対して行う。
//! 予約枠は、これらの操作の単位として予約とは別に読み書きされる。

/// 予約（単発予約）
pub mod entity;
/// Reservation集約のエラー型
pub mod errors;
/// Reservation集約のファクトリ
pub mod factory;
/// 予約枠
pub mod reserved_slot;
/// Reservation集約の値オブジェクト
pub mod value_objects;

pub use entity::{OneTimeReservation, Reservation};
pub use reserved_slot::ReservedSlot;
pub use value_objects::{ReservationId, ReservedSlotId};
