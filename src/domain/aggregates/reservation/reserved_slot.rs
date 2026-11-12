use super::entity::{Reservation, ReservationKind};
use super::errors::ReservationError;
use super::events::ReservationEvent;
use super::value_objects::*;
use crate::domain::common::{EmailAddress, TimePeriod};
use chrono::{DateTime, Utc};

/// 予約枠
///
/// 予約が実際に押さえている、ひとつながりの時間とリソース。属する予約をIDで参照する。
/// 単発予約は予約枠を1つ、定期予約は繰り返しの数だけ持つ。
///
/// 定期予約の予約枠を1つ変更・今で終了・取り消ししても、その予約枠は定期予約に属したままで
/// あり、他の予約枠には及ばない。
///
/// 時間の変更、今で終了、取り消しは予約枠に対して行い、どれも予約者本人だけができる。
/// 競合の判定、空きの計算、使われていない予約枠の検知も予約枠を単位とする。
#[derive(Debug, Clone, PartialEq)]
pub struct ReservedSlot {
    id: ReservedSlotId,
    belongs_to: BelongsTo,
    holder_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

/// 予約枠がどの予約に属するか
///
/// 単発予約の予約枠は、その予約と同じIDを持つ。そのため属する予約のIDを別に持たず、
/// 予約枠のIDから導く。定期予約の予約枠だけが、属する定期予約のIDを持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BelongsTo {
    /// 単発予約（予約のIDは予約枠のIDと同じ）
    OneTimeReservation,
    /// 定期予約
    RecurringReservation(ReservationId),
}

impl ReservedSlot {
    /// 単発予約の予約枠を、既存のIDで再構築する（リポジトリからの再構築用）
    ///
    /// 定期予約の予約枠として再構築する場合は、続けて[`Self::of_recurring_reservation`]を呼ぶ。
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
        Ok(Self::of_one_time_reservation(
            id,
            holder_email,
            time_period,
            resources,
            notes,
        ))
    }

    /// 単発予約の予約枠を組み立てる（予約側で検証済みの値から）
    pub(super) fn of_one_time_reservation(
        id: ReservedSlotId,
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Self {
        Self {
            id,
            belongs_to: BelongsTo::OneTimeReservation,
            holder_email,
            time_period,
            resources,
            notes,
        }
    }

    /// 定期予約の予約枠として位置づける（リポジトリからの再構築用）
    ///
    /// # Arguments
    /// * `reservation_id` - この予約枠が属する定期予約のID
    pub fn of_recurring_reservation(mut self, reservation_id: ReservationId) -> Self {
        self.belongs_to = BelongsTo::RecurringReservation(reservation_id);
        self
    }

    /// 予約枠IDを取得
    pub fn id(&self) -> &ReservedSlotId {
        &self.id
    }

    /// この予約枠が属する予約
    pub fn belongs_to(&self) -> &BelongsTo {
        &self.belongs_to
    }

    /// この予約枠が属する予約のID
    ///
    /// 単発予約の予約枠は、その予約と同じIDを持つ。
    pub fn reservation_id(&self) -> ReservationId {
        match &self.belongs_to {
            BelongsTo::OneTimeReservation => {
                ReservationId::from_string(self.id.as_str().to_string())
            }
            BelongsTo::RecurringReservation(id) => id.clone(),
        }
    }

    /// この予約枠が属する予約の種類
    pub fn reservation_kind(&self) -> ReservationKind {
        match self.belongs_to {
            BelongsTo::OneTimeReservation => ReservationKind::OneTime,
            BelongsTo::RecurringReservation(_) => ReservationKind::Recurring,
        }
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

    /// 予約枠の時間を変える
    ///
    /// 他の予約枠と重ならないかは、ここでは確かめない。それには他の予約枠を
    /// 読む必要があるため、呼び出し側が競合の判定（`ResourceConflictChecker`）を行う。
    ///
    /// # Errors
    /// `by`が予約者でない場合、`ReservationError::NotHolder`
    pub fn reschedule(
        &mut self,
        time_period: TimePeriod,
        by: &EmailAddress,
    ) -> Result<(), ReservationError> {
        self.ensure_held_by(by)?;
        self.time_period = time_period;
        Ok(())
    }

    /// 予約枠の備考を書き換える
    ///
    /// # Errors
    /// `by`が予約者でない場合、`ReservationError::NotHolder`
    pub fn change_notes(
        &mut self,
        notes: String,
        by: &EmailAddress,
    ) -> Result<(), ReservationError> {
        self.ensure_held_by(by)?;
        self.notes = Some(notes);
        Ok(())
    }

    /// 予約枠を`at`の時点で締める（今で終了）
    ///
    /// 取り消しが予約枠そのものを無かったことにするのに対し、今で終了は
    /// 開始から`at`までを使った事実として残し、残りの時間だけを空きに戻す。
    ///
    /// # Errors
    /// - `by`が予約者でない場合、`ReservationError::NotHolder`
    /// - `at`が開始時刻以前の場合、`ReservationError::NotYetStarted`
    /// - `at`が終了時刻以降の場合、`ReservationError::AlreadyEnded`
    pub fn end_early(
        &mut self,
        at: DateTime<Utc>,
        by: &EmailAddress,
    ) -> Result<(), ReservationError> {
        self.ensure_held_by(by)?;
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

    /// 予約枠を取り消す
    ///
    /// 予約枠を無かったことにする。単発予約の予約枠を取り消すと、その単発予約も無くなる。
    /// 定期予約の予約枠を取り消しても、定期予約と他の予約枠は残る。
    /// 取り消しによって起きた出来事を返す。永続化層から取り除くのは呼び出し側である。
    ///
    /// # Errors
    /// `by`が予約者でない場合、`ReservationError::NotHolder`
    pub fn cancel(self, by: &EmailAddress) -> Result<ReservationEvent, ReservationError> {
        self.ensure_held_by(by)?;
        Ok(match self.belongs_to {
            BelongsTo::OneTimeReservation => {
                ReservationEvent::Cancelled(Reservation::from_one_time_slot(&self))
            }
            BelongsTo::RecurringReservation(_) => ReservationEvent::SlotCancelled(self),
        })
    }

    /// `by`が予約者本人であることを確かめる
    fn ensure_held_by(&self, by: &EmailAddress) -> Result<(), ReservationError> {
        if by == &self.holder_email {
            Ok(())
        } else {
            Err(ReservationError::NotHolder { actor: by.clone() })
        }
    }
}

#[cfg(test)]
impl ReservedSlot {
    /// 新しい単発予約を作り、その予約枠を返す（テスト用）
    pub(crate) fn of_new_one_time_reservation(
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ReservationError> {
        let reservation = Reservation::one_time(holder_email, time_period, resources, notes)?;
        Ok(reservation
            .one_time_slot()
            .expect("単発予約は予約枠を1つ持つ"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn someone_else() -> EmailAddress {
        EmailAddress::new("other@example.com".to_string()).unwrap()
    }

    fn slot_running_from(start: DateTime<Utc>, end: DateTime<Utc>) -> ReservedSlot {
        ReservedSlot::of_new_one_time_reservation(
            holder(),
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

        slot.end_early(now, &holder()).unwrap();

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

        let error = slot.end_early(Utc::now(), &holder()).unwrap_err();

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

        let error = slot.end_early(start, &holder()).unwrap_err();

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

        let error = slot.end_early(Utc::now(), &holder()).unwrap_err();

        assert!(
            matches!(error, ReservationError::AlreadyEnded { .. }),
            "開く残り時間がない: {:?}",
            error
        );
    }

    #[test]
    fn ending_early_leaves_the_other_attributes_untouched() {
        let start = Utc::now() - Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(4));
        let id = slot.id().clone();
        let resources = slot.resources().to_vec();

        slot.end_early(start + Duration::minutes(30), &holder())
            .unwrap();

        assert_eq!(slot.id(), &id, "同じ予約枠であり続ける");
        assert_eq!(slot.resources(), resources.as_slice());
        assert_eq!(slot.holder_email(), &holder());
    }

    #[test]
    fn only_the_holder_can_change_end_or_cancel_a_slot() {
        let start = Utc::now() - Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(4));
        let untouched = slot.clone();
        let later = TimePeriod::new(start, start + Duration::hours(6)).unwrap();

        let refused = [
            slot.reschedule(later, &someone_else()).unwrap_err(),
            slot.change_notes("横取り".to_string(), &someone_else())
                .unwrap_err(),
            slot.end_early(Utc::now(), &someone_else()).unwrap_err(),
            slot.clone().cancel(&someone_else()).unwrap_err(),
        ];

        for error in refused {
            assert!(
                matches!(&error, ReservationError::NotHolder { actor } if actor == &someone_else()),
                "予約者以外の操作は断る: {:?}",
                error
            );
        }
        assert_eq!(slot, untouched, "断った操作は予約枠を変えない");
    }

    #[test]
    fn the_holder_can_reschedule_and_rewrite_the_notes() {
        let start = Utc::now() + Duration::hours(1);
        let mut slot = slot_running_from(start, start + Duration::hours(2));
        let later =
            TimePeriod::new(start + Duration::hours(3), start + Duration::hours(5)).unwrap();

        slot.reschedule(later.clone(), &holder()).unwrap();
        slot.change_notes("学習の続き".to_string(), &holder())
            .unwrap();

        assert_eq!(slot.time_period(), &later);
        assert_eq!(slot.notes().map(String::as_str), Some("学習の続き"));
    }

    #[test]
    fn ending_one_slot_early_keeps_it_part_of_its_recurring_reservation() {
        let start = Utc::now() - Duration::hours(1);
        let weekly = ReservationId::from_string("weekly-meeting".to_string());
        let mut slot = slot_running_from(start, start + Duration::hours(4))
            .of_recurring_reservation(weekly.clone());

        slot.end_early(start + Duration::minutes(30), &holder())
            .unwrap();

        assert_eq!(slot.reservation_kind(), ReservationKind::Recurring);
        assert_eq!(
            slot.reservation_id(),
            weekly,
            "締めるのはこの予約枠だけで、定期予約から外れるわけではない"
        );
    }

    #[test]
    fn cancelling_one_slot_of_a_recurring_reservation_leaves_the_reservation() {
        let start = Utc::now() + Duration::hours(1);
        let slot = slot_running_from(start, start + Duration::hours(2))
            .of_recurring_reservation(ReservationId::from_string("weekly".to_string()));

        let event = slot.clone().cancel(&holder()).unwrap();

        assert_eq!(event, ReservationEvent::SlotCancelled(slot));
    }

    #[test]
    fn cancelling_the_slot_of_a_one_time_reservation_cancels_the_reservation() {
        let start = Utc::now() + Duration::hours(1);
        let slot = slot_running_from(start, start + Duration::hours(2));
        let reservation_id = slot.reservation_id();

        let event = slot.cancel(&holder()).unwrap();

        match event {
            ReservationEvent::Cancelled(reservation) => {
                assert_eq!(reservation.id(), &reservation_id)
            }
            other => panic!("単発予約ごと取り消される: {:?}", other),
        }
    }
}
