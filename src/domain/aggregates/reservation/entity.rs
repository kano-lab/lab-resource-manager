use super::reserved_slot::ReservedSlot;
use super::value_objects::{ReservationId, Resource, TimePeriod};
use crate::domain::common::EmailAddress;

/// 予約
///
/// 予約者が、リソースを使う時間を前もって押さえる約束。作る・取り消すの単位である。
/// 予約は、実際に押さえている時間である予約枠を持つ。
#[derive(Debug, Clone, PartialEq)]
pub enum Reservation {
    /// 単発予約
    OneTime(OneTimeReservation),
}

impl Reservation {
    /// 予約IDを取得
    pub fn id(&self) -> &ReservationId {
        match self {
            Self::OneTime(reservation) => reservation.id(),
        }
    }

    /// 予約者のメールアドレスを取得
    pub fn holder_email(&self) -> &EmailAddress {
        match self {
            Self::OneTime(reservation) => reservation.holder_email(),
        }
    }

    /// 押さえるリソースを取得
    pub fn resources(&self) -> &[Resource] {
        match self {
            Self::OneTime(reservation) => reservation.resources(),
        }
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        match self {
            Self::OneTime(reservation) => reservation.notes(),
        }
    }
}

/// 単発予約
///
/// 期間が1つに決まっている予約。予約枠をちょうど1つ持ち、その予約枠と同じ内容を持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct OneTimeReservation {
    id: ReservationId,
    holder_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

impl OneTimeReservation {
    /// 単発予約を、その唯一の予約枠から組み立てる
    pub fn from_slot(slot: &ReservedSlot) -> Self {
        Self {
            id: slot.reservation_id().clone(),
            holder_email: slot.holder_email().clone(),
            time_period: slot.time_period().clone(),
            resources: slot.resources().clone(),
            notes: slot.notes().cloned(),
        }
    }

    /// 予約IDを取得
    pub fn id(&self) -> &ReservationId {
        &self.id
    }

    /// 予約者のメールアドレスを取得
    pub fn holder_email(&self) -> &EmailAddress {
        &self.holder_email
    }

    /// 期間を取得
    pub fn time_period(&self) -> &TimePeriod {
        &self.time_period
    }

    /// 押さえるリソースを取得
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }
}
