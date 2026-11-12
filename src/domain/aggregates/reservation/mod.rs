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
//! | 予約枠 | [`ReservedSlot`] | 予約の中の回。予約が実際に押さえている、ひとつながりの時間。予約が日程から求める |
//! | 予約枠ID | [`ReservedSlotId`] | 予約の外から予約枠を指すID。予約ID ＋ 回の鍵（[`OccurrenceKey`]） |
//! | 時間の変更 | [`Reservation::reschedule_slot`] | 予約枠の期間を変えること |
//! | 今で終了 | [`Reservation::end_slot_early`] | 進行中の予約枠を今の時点で締め、残りの時間を空きに戻すこと。使った分は予約枠として残る |
//! | 取り消し | [`Reservation::cancel`]、[`Reservation::cancel_slot`] | 予約や予約枠を無かったことにすること。単発予約の予約枠を取り消すと、その予約も取り消される |
//! | 版 | [`Version`] | 保存先での予約の版。読んだときと同じ版のときだけ書き戻せる |
//! | 空き | `ResourceAvailability` | どの予約枠とも重ならない時間 |
//! | 実利用 | `ObservedUsage` | 実際にリソースが使われている様子。予約とは別に観測し、予約枠と突き合わせる |
//!
//! 「予約枠」は、予約サービスで一般に言う空き枠（提供側が用意した受付の枠）のことではない。
//! 予約者が予約によって押さえた時間を指す。
//!
//! ## 集約
//!
//! 予約が集約の根であり、予約枠はその中の回である。予約は予約枠をフィールドとして持たず、
//! 自分の日程から求める。予約枠への操作もすべて予約を通し、決まり（予約者本人だけ、
//! 取り消し済みなら断る、日程に無い回は無い）は予約が一か所で守る。
//!
//! 書き込みは予約を単位とし、リポジトリは予約を読んで（`find`）、書き戻す（`save`）。
//! 予約枠を期間で引く読み取り（競合の判定、空きの計算、使われていない予約枠の検知、一覧）は、
//! 予約をまたぐため、読み取り専用の写し（`SlotView`）を別の口から引く。
//!
//! 予約に起きたことは、出来事（[`ReservationEvent`]）として表す。

/// 予約
pub mod entity;
/// Reservation集約のエラー型
pub mod errors;
/// 予約に起きた出来事
pub mod events;
/// Reservation集約のファクトリ
pub mod factory;
/// 定期予約の日程
pub mod recurring;
/// 予約枠
pub mod reserved_slot;
/// Reservation集約の値オブジェクト
pub mod value_objects;

pub use entity::{Reservation, ReservationStatus, Schedule};
pub use events::ReservationEvent;
pub use recurring::{Exception, ExceptionChange, RecurringSchedule};
pub use reserved_slot::ReservedSlot;
pub use value_objects::{
    OccurrenceKey, RecurrenceEnd, RecurrenceFrequency, RecurrenceRule, ReservationId,
    ReservedSlotId, Version,
};
