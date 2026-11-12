use super::entity::Reservation;
use super::reserved_slot::ReservedSlot;
use super::value_objects::Resource;
use crate::domain::common::EmailAddress;

/// 予約に起きた出来事
///
/// 出来事は予約を単位とする。単発予約でも定期予約でも、作られた・変わった・
/// 取り消されたは1件の出来事である。定期予約の予約枠が1つだけ変わったときに限り、
/// その予約枠を単位とする（`Slot…`）。
///
/// # 出来事の出どころ
/// 予約や予約枠の取り消しのように、操作は起こした出来事を呼び出し元へ返す。
///
/// 予約チャンネルへ知らせる出来事は、それとは別に、保存先の変化から読み取る
/// （`ReservationChangeInterpreter`）。カレンダーは人の手でも直接編集されるため、
/// アプリを通った操作の戻り値だけでは、起きたことを取りこぼす。
#[derive(Debug, Clone, PartialEq)]
pub enum ReservationEvent {
    /// 予約された
    Reserved(Reservation),
    /// 予約が変わった
    ///
    /// 単発予約では、その予約枠の時間や備考の変更。定期予約では、時間帯や繰り返しの規則の
    /// 変更、途中からの打ち切りなど、複数の予約枠に及ぶ変更。
    Changed(Reservation),
    /// 単発予約の予約枠が今で終了した
    ///
    /// 開始から終了した時点までは、使った時間として予約枠に残る。
    EndedEarly(Reservation),
    /// 予約が取り消された（定期予約なら、そのすべての予約枠が無くなった）
    Cancelled(Reservation),
    /// 定期予約の予約枠が1つだけ変わった
    SlotChanged(ReservedSlot),
    /// 定期予約の予約枠が1つだけ今で終了した
    SlotEndedEarly(ReservedSlot),
    /// 定期予約の予約枠が1つだけ取り消された
    SlotCancelled(ReservedSlot),
}

impl ReservationEvent {
    /// 出来事が起きた予約の予約者
    pub fn holder_email(&self) -> &EmailAddress {
        match self {
            Self::Reserved(reservation)
            | Self::Changed(reservation)
            | Self::EndedEarly(reservation)
            | Self::Cancelled(reservation) => reservation.holder_email(),
            Self::SlotChanged(slot) | Self::SlotEndedEarly(slot) | Self::SlotCancelled(slot) => {
                slot.holder_email()
            }
        }
    }

    /// 出来事が起きた予約（または予約枠）が押さえるリソース
    pub fn resources(&self) -> &[Resource] {
        match self {
            Self::Reserved(reservation)
            | Self::Changed(reservation)
            | Self::EndedEarly(reservation)
            | Self::Cancelled(reservation) => reservation.resources(),
            Self::SlotChanged(slot) | Self::SlotEndedEarly(slot) | Self::SlotCancelled(slot) => {
                slot.resources()
            }
        }
    }

    /// 出来事の対象（予約または予約枠）のID
    pub fn subject_id(&self) -> &str {
        match self {
            Self::Reserved(reservation)
            | Self::Changed(reservation)
            | Self::EndedEarly(reservation)
            | Self::Cancelled(reservation) => reservation.id().as_str(),
            Self::SlotChanged(slot) | Self::SlotEndedEarly(slot) | Self::SlotCancelled(slot) => {
                slot.id().as_str()
            }
        }
    }
}
