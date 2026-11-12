use super::errors::ReservationError;
use super::value_objects::*;
use crate::domain::common::EmailAddress;
use chrono::{DateTime, Utc};

/// 予約枠
///
/// 予約が実際に押さえている、ひとつながりの時間。リソースと期間を持つ。
/// 単発予約は予約枠を1つ持つ。
///
/// 競合の判定、空きの計算、使われていない予約枠の検知、今で終了は、予約枠に対して行う。
#[derive(Debug, Clone, PartialEq)]
pub struct ReservedSlot {
    id: ReservedSlotId,
    reservation_id: ReservationId,
    holder_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

impl ReservedSlot {
    /// 単発予約を新しく作り、その予約枠を返す
    ///
    /// 単発予約と予約枠は1対1であり、同じ識別子を持つ。
    ///
    /// # Arguments
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 期間
    /// * `resources` - 押さえるリソース
    /// * `notes` - 備考（オプション）
    ///
    /// # Errors
    /// リソースが空の場合、`ReservationError::NoResourceItems`を返す
    pub fn new(
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ReservationError> {
        Self::reconstruct(
            ReservedSlotId::new(),
            holder_email,
            time_period,
            resources,
            notes,
        )
    }

    /// 単発予約の予約枠を、既存のIDで再構築する（リポジトリからの再構築用）
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
            reservation_id: ReservationId::from_string(id.as_str().to_string()),
            id,
            holder_email,
            time_period,
            resources,
            notes,
        })
    }

    /// 予約枠IDを取得
    pub fn id(&self) -> &ReservedSlotId {
        &self.id
    }

    /// この予約枠が属する予約のIDを取得
    pub fn reservation_id(&self) -> &ReservationId {
        &self.reservation_id
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
    pub fn resources(&self) -> &Vec<Resource> {
        &self.resources
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }

    /// 期間を更新する
    pub fn update_time_period(&mut self, new_time_period: TimePeriod) {
        self.time_period = new_time_period;
    }

    /// 備考を更新する
    pub fn update_notes(&mut self, notes: String) {
        self.notes = Some(notes);
    }

    /// 予約枠を`at`の時点で締める（今で終了）
    ///
    /// 取り消しが予約枠そのものを無かったことにするのに対し、今で終了は
    /// 開始から`at`までを使った事実として残し、残りの時間だけを空きに戻す。
    ///
    /// # Errors
    /// - `at`が開始時刻以前の場合、`ReservationError::NotYetStarted`
    /// - `at`が終了時刻以降の場合、`ReservationError::AlreadyEnded`
    pub fn end_early(&mut self, at: DateTime<Utc>) -> Result<(), ReservationError> {
        if at <= self.time_period.start() {
            return Err(ReservationError::NotYetStarted {
                start: self.time_period.start(),
                at,
            });
        }
        if at >= self.time_period.end() {
            return Err(ReservationError::AlreadyEnded {
                end: self.time_period.end(),
                at,
            });
        }

        self.time_period = TimePeriod::new(self.time_period.start(), at)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn slot_running_from(start: DateTime<Utc>, end: DateTime<Utc>) -> ReservedSlot {
        ReservedSlot::new(
            EmailAddress::new("holder@example.com".to_string()).unwrap(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[test]
    fn ending_early_shortens_the_slot_to_that_moment() {
        let start = Utc::now() - Duration::hours(1);
        let end = start + Duration::hours(4);
        let mut slot = slot_running_from(start, end);
        let now = start + Duration::hours(1);

        slot.end_early(now).unwrap();

        assert_eq!(
            slot.time_period().start(),
            start,
            "使い始めた時刻は実績として残る"
        );
        assert_eq!(slot.time_period().end(), now);
    }

    #[test]
    fn a_slot_that_has_not_started_cannot_end_early() {
        let start = Utc::now() + Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(2));

        let error = slot.end_early(Utc::now()).unwrap_err();

        assert!(
            matches!(error, ReservationError::NotYetStarted { .. }),
            "使った時間がない予約枠は締められない（取り消しの領分）: {:?}",
            error
        );
    }

    #[test]
    fn ending_exactly_at_the_start_leaves_nothing_to_record() {
        let start = Utc::now() - Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(2));

        let error = slot.end_early(start).unwrap_err();

        assert!(
            matches!(error, ReservationError::NotYetStarted { .. }),
            "長さ0の予約枠は残せない: {:?}",
            error
        );
    }

    #[test]
    fn a_slot_that_has_already_ended_cannot_end_early() {
        let end = Utc::now() - Duration::hours(1);
        let mut slot = slot_running_from(end - Duration::hours(2), end);

        let error = slot.end_early(Utc::now()).unwrap_err();

        assert!(
            matches!(error, ReservationError::AlreadyEnded { .. }),
            "開く残り時間がない: {:?}",
            error
        );
    }

    #[test]
    fn a_new_slot_is_the_only_slot_of_a_new_one_time_reservation() {
        let start = Utc::now();
        let slot = slot_running_from(start, start + Duration::hours(1));

        assert_eq!(
            slot.reservation_id().as_str(),
            slot.id().as_str(),
            "単発予約と予約枠は同じ識別子を持つ"
        );
    }

    #[test]
    fn ending_early_leaves_the_other_attributes_untouched() {
        let start = Utc::now() - Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(4));
        let id = slot.id().clone();
        let resources = slot.resources().clone();

        slot.end_early(start + Duration::minutes(30)).unwrap();

        assert_eq!(slot.id(), &id, "同じ予約枠であり続ける");
        assert_eq!(slot.resources(), &resources);
        assert_eq!(slot.holder_email().as_str(), "holder@example.com");
    }
}
