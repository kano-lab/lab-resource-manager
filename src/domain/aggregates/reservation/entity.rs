use super::reserved_slot::ReservedSlot;
use super::value_objects::{RecurrenceRule, ReservationId, Resource, TimePeriod};
use crate::domain::common::EmailAddress;

/// 予約の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationKind {
    /// 単発予約
    OneTime,
    /// 定期予約
    Recurring,
}

/// 予約
///
/// 予約者が、リソースを使う時間を前もって押さえる約束。作る・取り消すの単位である。
/// 定期予約を1件作ることは利用者にとって1つの行為であり、予約枠の数だけの行為ではない。
#[derive(Debug, Clone, PartialEq)]
pub enum Reservation {
    /// 単発予約
    OneTime(OneTimeReservation),
    /// 定期予約
    Recurring(RecurringReservation),
}

impl Reservation {
    /// 予約IDを取得
    pub fn id(&self) -> &ReservationId {
        match self {
            Self::OneTime(reservation) => reservation.id(),
            Self::Recurring(reservation) => reservation.id(),
        }
    }

    /// 予約の種類を取得
    pub fn kind(&self) -> ReservationKind {
        match self {
            Self::OneTime(_) => ReservationKind::OneTime,
            Self::Recurring(_) => ReservationKind::Recurring,
        }
    }

    /// 予約者のメールアドレスを取得
    pub fn holder_email(&self) -> &EmailAddress {
        match self {
            Self::OneTime(reservation) => reservation.holder_email(),
            Self::Recurring(reservation) => reservation.holder_email(),
        }
    }

    /// 押さえるリソースを取得
    pub fn resources(&self) -> &[Resource] {
        match self {
            Self::OneTime(reservation) => reservation.resources(),
            Self::Recurring(reservation) => reservation.resources(),
        }
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        match self {
            Self::OneTime(reservation) => reservation.notes(),
            Self::Recurring(reservation) => reservation.notes(),
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

/// 定期予約
///
/// 時間帯と繰り返しの規則で、何度も押さえる予約。繰り返しの数だけ予約枠を持つ。
///
/// 時間帯は、初回の予約枠の期間から読み取る。繰り返しの規則が各予約枠の日付を決め、
/// 初回の予約枠の時刻と長さがすべての予約枠の時刻と長さを決める（iCalendarと同じ考え方）。
#[derive(Debug, Clone, PartialEq)]
pub struct RecurringReservation {
    id: ReservationId,
    holder_email: EmailAddress,
    resources: Vec<Resource>,
    notes: Option<String>,
    /// 初回の予約枠の期間（把握できない場合は、把握できる最も早い予約枠の期間）
    first_slot_period: TimePeriod,
    /// 繰り返しの規則。永続化層の表現を読み取れなかった場合は`None`
    rule: Option<RecurrenceRule>,
}

impl RecurringReservation {
    /// 定期予約を、初回の予約枠と繰り返しの規則から組み立てる
    ///
    /// 予約者・リソース・備考・時間帯は予約枠から引き継ぐ。
    ///
    /// # Arguments
    /// * `id` - 予約ID
    /// * `first_slot` - 初回の予約枠（把握できない場合は、把握できる最も早い予約枠）
    /// * `rule` - 繰り返しの規則（読み取れなかった場合は`None`）
    pub fn from_first_slot(
        id: ReservationId,
        first_slot: &ReservedSlot,
        rule: Option<RecurrenceRule>,
    ) -> Self {
        Self {
            id,
            holder_email: first_slot.holder_email().clone(),
            resources: first_slot.resources().clone(),
            notes: first_slot.notes().cloned(),
            first_slot_period: first_slot.time_period().clone(),
            rule,
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

    /// 押さえるリソースを取得
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }

    /// 初回の予約枠の期間を取得（時間帯はここから読み取る）
    pub fn first_slot_period(&self) -> &TimePeriod {
        &self.first_slot_period
    }

    /// 繰り返しの規則を取得（読み取れなかった場合は`None`）
    pub fn rule(&self) -> Option<&RecurrenceRule> {
        self.rule.as_ref()
    }
}
