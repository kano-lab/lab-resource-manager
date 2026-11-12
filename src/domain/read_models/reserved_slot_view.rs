use crate::domain::aggregates::reservation::{
    Reservation, ReservationId, ReservedSlot, ReservedSlotId, errors::ReservationError,
    value_objects::Resource,
};
use crate::domain::common::{EmailAddress, TimePeriod};

/// 予約枠の写し（読み取りモデル）
///
/// 予約枠を期間で引くための、予約者とリソースを添えた写し。書き込みには使わない。
/// 予約枠を変えるときは、属する予約を読んで操作する（`ReservationRepository`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ReservedSlotView {
    id: ReservedSlotId,
    holder_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

impl ReservedSlotView {
    /// 予約枠の写しを、既存のIDで再構築する（読み取りの口からの再構築用）
    ///
    /// 予約枠を新しく作る手段ではない。予約枠は予約を作ることで生まれる
    /// （[`Reservation::one_time`]）。
    ///
    /// # Arguments
    /// * `id` - 既存の予約枠ID
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 期間
    /// * `resources` - 押さえるリソース
    /// * `notes` - 備考（オプション）
    ///
    /// # Errors
    /// リソースが空の場合、`ReservationError::NoResourceItems`を返す
    pub fn reconstruct(
        id: ReservedSlotId,
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ReservationError> {
        if resources.is_empty() {
            return Err(ReservationError::NoResourceItems);
        }
        Ok(Self {
            id,
            holder_email,
            time_period,
            resources,
            notes,
        })
    }

    /// 予約とその予約枠から写しを作る
    pub fn of(reservation: &Reservation, slot: &ReservedSlot) -> Self {
        Self {
            id: reservation.slot_id(slot.key().clone()),
            holder_email: reservation.holder_email().clone(),
            time_period: slot.time_period().clone(),
            resources: reservation.resources().to_vec(),
            notes: slot.notes().cloned(),
        }
    }

    /// 予約枠IDを取得
    pub fn id(&self) -> &ReservedSlotId {
        &self.id
    }

    /// この予約枠が属する予約のID
    ///
    /// 単発予約の予約枠は、その予約と同じIDを持つ。
    pub fn reservation_id(&self) -> &ReservationId {
        self.id.reservation_id()
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

#[cfg(test)]
impl ReservedSlotView {
    /// 新しい単発予約を作り、その予約枠の写しを返す（テスト用）
    pub(crate) fn of_new_one_time_reservation(
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ReservationError> {
        use crate::domain::aggregates::reservation::OccurrenceKey;
        let reservation = Reservation::one_time(holder_email, time_period, resources, notes)?;
        let slot = reservation
            .slot(&OccurrenceKey::Only)
            .expect("単発予約は予約枠を1つ持つ");
        Ok(Self::of(&reservation, &slot))
    }
}
