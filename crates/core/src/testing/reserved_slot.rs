//! テストで使う予約枠の写し

use crate::domain::aggregates::reservation::errors::ReservationError;
use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::aggregates::reservation::{OccurrenceKey, Reservation};
use crate::domain::common::{EmailAddress, TimePeriod};
use crate::domain::read_models::ReservedSlotView;

/// 新しい単発予約を作り、その予約枠の写しを返す
///
/// 通知や表示の組み立てを確かめるときに、予約を保存せずに予約枠の写しだけを得るために使う。
///
/// # Errors
/// 予約として成り立たない場合（リソースが空など）
pub fn reserved_slot_of_new_one_time_reservation(
    holder_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
) -> Result<ReservedSlotView, ReservationError> {
    let reservation = Reservation::one_time(holder_email, time_period, resources, notes)?;
    let slot = reservation
        .slot(&OccurrenceKey::Only)
        .expect("単発予約は予約枠を1つ持つ");
    Ok(ReservedSlotView::of(&reservation, &slot))
}
