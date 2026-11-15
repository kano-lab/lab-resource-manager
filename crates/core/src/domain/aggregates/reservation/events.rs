use super::entity::Reservation;
use super::reserved_slot::ReservedSlot;
use super::value_objects::Resource;
use crate::domain::common::EmailAddress;

/// 予約に起きた出来事
///
/// # 出来事の出どころ
/// 予約枠の取り消しのように、操作は起こした出来事を呼び出し元へ返す。
///
/// 予約チャンネルへ知らせる出来事は、それとは別に、保存先の変化から読み取る
/// （`ReservationChangeInterpreter`）。カレンダーは人の手でも直接編集されるため、
/// アプリを通った操作の戻り値だけでは、起きたことを取りこぼす。
#[derive(Debug, Clone, PartialEq)]
pub enum ReservationEvent {
    /// 予約された
    Reserved(Reservation),
    /// 予約枠の時間や備考が変わった
    Changed(Reservation),
    /// 進行中の予約枠が今で終了した
    ///
    /// 開始から終了した時点までは、使った時間として予約枠に残る。
    EndedEarly(Reservation),
    /// 予約が取り消された
    Cancelled(Reservation),
    /// 定期予約の1つの予約枠の時間や備考が変わった（ほかの予約枠はそのまま）
    SlotChanged {
        /// 変わったあとの予約
        reservation: Reservation,
        /// 変わったあとの予約枠
        slot: ReservedSlot,
    },
    /// 定期予約の進行中の予約枠が今で終了した（ほかの予約枠はそのまま）
    SlotEndedEarly {
        /// 終了したあとの予約
        reservation: Reservation,
        /// 締めたあとの予約枠
        slot: ReservedSlot,
    },
    /// 定期予約の1つの予約枠が取り消された（予約とほかの予約枠は残る）
    SlotCancelled {
        /// 取り消したあとの予約
        reservation: Reservation,
        /// 取り消された予約枠（取り消す前の姿）
        slot: ReservedSlot,
    },
}

impl ReservationEvent {
    /// 出来事が起きた予約
    pub fn reservation(&self) -> &Reservation {
        match self {
            Self::Reserved(reservation)
            | Self::Changed(reservation)
            | Self::EndedEarly(reservation)
            | Self::Cancelled(reservation)
            | Self::SlotChanged { reservation, .. }
            | Self::SlotEndedEarly { reservation, .. }
            | Self::SlotCancelled { reservation, .. } => reservation,
        }
    }

    /// 出来事が起きた予約枠（定期予約の1つの予約枠に起きた出来事なら）
    pub fn slot(&self) -> Option<&ReservedSlot> {
        match self {
            Self::SlotChanged { slot, .. }
            | Self::SlotEndedEarly { slot, .. }
            | Self::SlotCancelled { slot, .. } => Some(slot),
            Self::Reserved(_) | Self::Changed(_) | Self::EndedEarly(_) | Self::Cancelled(_) => None,
        }
    }

    /// 出来事が起きた予約の予約者
    pub fn holder_email(&self) -> &EmailAddress {
        self.reservation().holder_email()
    }

    /// 出来事が起きた予約が押さえるリソース
    pub fn resources(&self) -> &[Resource] {
        self.reservation().resources()
    }

    /// 出来事の対象のID（ログや通知の宛先の判断に使う）
    pub fn subject_id(&self) -> &str {
        self.reservation().id().as_str()
    }
}
