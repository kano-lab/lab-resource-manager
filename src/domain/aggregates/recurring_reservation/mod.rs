//! # RecurringReservation集約
//!
//! 決まった間隔で繰り返される予約（定期予約）を表す集約です。
//!
//! 定期予約の各回は`ResourceUsage`として扱われ、競合の判定や実利用との突き合わせは
//! これまでどおり回ごとに行われます。この集約が表すのは、それらの回を生み出している
//! 「毎週月曜の10時から」といった一つの取り決めです。

/// RecurringReservation集約のエンティティ定義
pub mod entity;
/// RecurringReservation集約の値オブジェクト
pub mod value_objects;

pub use entity::RecurringReservation;
pub use value_objects::{
    RecurrenceEnd, RecurrenceFrequency, RecurrenceRule, RecurringReservationId,
};
