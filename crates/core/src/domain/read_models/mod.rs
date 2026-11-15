//! # 読み取りモデル
//!
//! 書き込みの単位（集約）とは別の形で、読み取りのために持つ写し。
//!
//! 予約は集約の根として予約者・リソース・日程を持ち、予約枠はその中の回である。
//! ところが競合の判定、空きの計算、使われていない予約枠の検知、一覧は、どれも
//! 「この期間に重なる予約枠を、予約をまたいで全部」を引く。これを予約集約から引くと
//! すべての予約を読むことになるため、予約枠ごとの写し（[`ReservedSlotView`](crate::domain::read_models::ReservedSlotView)）を別の口
//! （`ReservedSlotQuery`）から引く。

/// 予約枠の写し
pub mod reserved_slot_view;

pub use reserved_slot_view::ReservedSlotView;
