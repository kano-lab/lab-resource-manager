//! # 予約と予約枠
//!
//! 研究室のリソース（GPU、部屋）を使う時間を、前もって押さえる仕組みを扱う。
//!
//! ## 言葉
//!
//! この一覧を言葉の正本とする。ユーザーガイドや通知の文言もこれに合わせる。
//!
//! | 言葉 | 英語 | 意味 |
//! | --- | --- | --- |
//! | 予約 | [`Reservation`] | 予約者が、リソースを使う時間を前もって押さえる約束。作る・取り消すの単位 |
//! | 予約者 | holder | 予約をした人。予約とその予約枠を変えられるのは予約者本人だけ |
//! | 日程 | [`Schedule`] | 予約がいつ時間を押さえるか |
//! | 単発予約 | [`Schedule::OneTime`] | 日程が期間1つの予約。予約枠を1つ持ち、その予約枠は予約と同じIDを持つ |
//! | 予約枠 | [`SlotView`](crate::domain::read_models::SlotView) | 予約が実際に押さえている、ひとつながりの時間とリソース |
//! | 時間の変更 | [`SlotView::reschedule`](crate::domain::read_models::SlotView::reschedule) | 予約枠の期間を変えること |
//! | 今で終了 | [`SlotView::end_early`](crate::domain::read_models::SlotView::end_early) | 進行中の予約枠を今の時点で締め、残りの時間を空きに戻すこと。使った分は予約枠として残る |
//! | 取り消し | [`SlotView::cancel`](crate::domain::read_models::SlotView::cancel) | 予約枠を無かったことにすること。単発予約の予約枠を取り消すと、その予約も無くなる |
//! | 空き | `ResourceAvailability` | どの予約枠とも重ならない時間 |
//! | 実利用 | `ObservedUsage` | 実際にリソースが使われている様子。予約とは別に観測し、予約枠と突き合わせる |
//!
//! 「予約枠」は、予約サービスで一般に言う空き枠（提供側が用意した受付の枠）のことではない。
//! 予約者が予約によって押さえた時間を指す。
//!
//! ## 集約
//!
//! 予約と予約枠は、それぞれ別の集約である。予約枠は、属する予約をIDで参照する。
//!
//! - 予約は、作る・取り消すの単位である。新しい予約は予約のリポジトリへ加え、
//!   その予約枠は予約枠のリポジトリから読めるようになる
//! - 予約枠は、時間の変更・今で終了・取り消しの単位である。競合の判定、空きの計算、
//!   使われていない予約枠の検知も予約枠に対して行う
//!
//! 予約や予約枠に起きたことは、出来事（[`ReservationEvent`]）として表す。

/// 予約
pub mod entity;
/// Reservation集約のエラー型
pub mod errors;
/// 予約に起きた出来事
pub mod events;
/// Reservation集約のファクトリ
pub mod factory;
/// Reservation集約の値オブジェクト
pub mod value_objects;

pub use entity::{Reservation, Schedule};
pub use events::ReservationEvent;
pub use value_objects::{ReservationId, ReservedSlotId};
