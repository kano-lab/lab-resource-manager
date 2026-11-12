use super::errors::ReservationError;
use super::events::ReservationEvent;
use super::reserved_slot::ReservedSlot;
use super::value_objects::{RecurrenceRule, ReservationId, ReservedSlotId, Resource};
use crate::domain::common::{EmailAddress, TimePeriod};

/// 予約
///
/// 予約者が、リソースを使う時間を前もって押さえる約束。作る・取り消すの単位である。
/// 予約者・リソース・備考はどの予約にもあり、いつ押さえるかは日程（[`Schedule`]）が表す。
/// 定期予約を1件作ることは利用者にとって1つの行為であり、予約枠の数だけの行為ではない。
/// 予約が実際に押さえている時間は、予約枠（[`ReservedSlot`]）として別に扱う。
#[derive(Debug, Clone, PartialEq)]
pub struct Reservation {
    id: ReservationId,
    holder_email: EmailAddress,
    resources: Vec<Resource>,
    notes: Option<String>,
    schedule: Schedule,
}

/// 予約の日程（いつ時間を押さえるか）
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    /// 単発: 期間が1つに決まっている
    ///
    /// 単発予約は予約枠を1つだけ持ち、その予約枠は予約と同じIDで識別される。
    OneTime(TimePeriod),
    /// 定期: 時間帯と繰り返しの規則で、何度も時間を押さえる
    ///
    /// 定期予約は繰り返しの数だけ予約枠を持つ。時間帯は初回の予約枠の期間から読み取る。
    /// 繰り返しの規則が各予約枠の日付を決め、初回の予約枠の時刻と長さがすべての予約枠の
    /// 時刻と長さを決める（iCalendarと同じ考え方）。
    Recurring {
        /// 初回の予約枠の期間（把握できない場合は、把握できる最も早い予約枠の期間）
        first_slot: TimePeriod,
        /// 繰り返しの規則。永続化層の表現を読み取れなかった場合は`None`
        rule: Option<RecurrenceRule>,
    },
}

/// 予約の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationKind {
    /// 単発予約
    OneTime,
    /// 定期予約
    Recurring,
}

impl Schedule {
    /// この日程の予約の種類
    pub fn kind(&self) -> ReservationKind {
        match self {
            Self::OneTime(_) => ReservationKind::OneTime,
            Self::Recurring { .. } => ReservationKind::Recurring,
        }
    }
}

impl Reservation {
    /// 単発予約を新しく作る
    ///
    /// # Arguments
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 押さえる期間
    /// * `resources` - 押さえるリソース
    /// * `notes` - 備考（オプション）
    ///
    /// # Errors
    /// リソースが空の場合、`ReservationError::NoResourceItems`を返す
    pub fn one_time(
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ReservationError> {
        Self::reconstruct(
            ReservationId::new(),
            holder_email,
            resources,
            notes,
            Schedule::OneTime(time_period),
        )
    }

    /// 既存のIDで予約を再構築する（リポジトリからの再構築用）
    ///
    /// # Errors
    /// リソースが空の場合、`ReservationError::NoResourceItems`を返す
    pub fn reconstruct(
        id: ReservationId,
        holder_email: EmailAddress,
        resources: Vec<Resource>,
        notes: Option<String>,
        schedule: Schedule,
    ) -> Result<Self, ReservationError> {
        if resources.is_empty() {
            return Err(ReservationError::NoResourceItems);
        }
        Ok(Self {
            id,
            holder_email,
            resources,
            notes,
            schedule,
        })
    }

    /// 定期予約を、初回の予約枠と繰り返しの規則から読み取る（リポジトリからの再構築用）
    ///
    /// 予約者・リソース・備考・時間帯は予約枠から引き継ぐ。
    ///
    /// # Arguments
    /// * `id` - 予約ID
    /// * `first_slot` - 初回の予約枠（把握できない場合は、把握できる最も早い予約枠）
    /// * `rule` - 繰り返しの規則（読み取れなかった場合は`None`）
    pub fn recurring_from_first_slot(
        id: ReservationId,
        first_slot: &ReservedSlot,
        rule: Option<RecurrenceRule>,
    ) -> Self {
        Self {
            id,
            holder_email: first_slot.holder_email().clone(),
            resources: first_slot.resources().to_vec(),
            notes: first_slot.notes().cloned(),
            schedule: Schedule::Recurring {
                first_slot: first_slot.time_period().clone(),
                rule,
            },
        }
    }

    /// 単発予約を、その唯一の予約枠から読み取る
    ///
    /// 単発予約と予約枠は1対1で、同じIDと内容を持つ。永続化層はこの2つを
    /// 1つのレコードとして持つため、予約枠から予約を組み立てられる。
    /// 定期予約の予約枠には使わない（[`Self::recurring_from_first_slot`]を使う）。
    pub fn from_one_time_slot(slot: &ReservedSlot) -> Self {
        Self {
            id: slot.reservation_id(),
            holder_email: slot.holder_email().clone(),
            resources: slot.resources().to_vec(),
            notes: slot.notes().cloned(),
            schedule: Schedule::OneTime(slot.time_period().clone()),
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

    /// 日程を取得
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// 予約の種類を取得
    pub fn kind(&self) -> ReservationKind {
        self.schedule.kind()
    }

    /// 予約を取り消す
    ///
    /// 予約を無かったことにする。定期予約なら、そのすべての予約枠が無くなる。
    /// 取り消しによって起きた出来事を返す。永続化層から取り除くのは呼び出し側である。
    ///
    /// # Errors
    /// `by`が予約者でない場合、`ReservationError::NotHolder`
    pub fn cancel(self, by: &EmailAddress) -> Result<ReservationEvent, ReservationError> {
        if by != &self.holder_email {
            return Err(ReservationError::NotHolder { actor: by.clone() });
        }
        Ok(ReservationEvent::Cancelled(self))
    }

    /// 単発予約の予約枠を取得する
    ///
    /// 単発予約は予約枠を1つだけ持ち、その予約枠は予約と同じIDを持つ。
    /// 定期予約の予約枠は繰り返しの1回ごとに永続化層が決めるため、ここでは返さない。
    pub fn one_time_slot(&self) -> Option<ReservedSlot> {
        match &self.schedule {
            Schedule::OneTime(time_period) => Some(ReservedSlot::of_one_time_reservation(
                ReservedSlotId::from_string(self.id.as_str().to_string()),
                self.holder_email.clone(),
                time_period.clone(),
                self.resources.clone(),
                self.notes.clone(),
            )),
            Schedule::Recurring { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use chrono::{Duration, Utc};

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn gpu() -> Resource {
        Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()))
    }

    fn period() -> TimePeriod {
        let start = Utc::now();
        TimePeriod::new(start, start + Duration::hours(1)).unwrap()
    }

    #[test]
    fn a_new_one_time_reservation_holds_exactly_one_slot_with_its_own_id() {
        let reservation =
            Reservation::one_time(holder(), period(), vec![gpu()], Some("学習".to_string()))
                .unwrap();

        let slot = reservation.one_time_slot().unwrap();

        assert_eq!(slot.id().as_str(), reservation.id().as_str());
        assert_eq!(&slot.reservation_id(), reservation.id());
        assert_eq!(slot.holder_email(), reservation.holder_email());
        assert_eq!(slot.resources(), reservation.resources());
        assert_eq!(slot.notes(), reservation.notes());
        assert_eq!(
            reservation.schedule(),
            &Schedule::OneTime(slot.time_period().clone())
        );
    }

    #[test]
    fn a_reservation_must_hold_at_least_one_resource() {
        let error = Reservation::one_time(holder(), period(), vec![], None).unwrap_err();

        assert!(matches!(error, ReservationError::NoResourceItems));
    }

    #[test]
    fn a_recurring_reservation_takes_its_time_of_day_from_the_first_slot() {
        let first =
            Reservation::one_time(holder(), period(), vec![gpu()], Some("輪講".to_string()))
                .unwrap()
                .one_time_slot()
                .unwrap();
        let id = ReservationId::from_string("weekly".to_string());

        let recurring = Reservation::recurring_from_first_slot(id.clone(), &first, None);

        assert_eq!(recurring.id(), &id);
        assert_eq!(recurring.kind(), ReservationKind::Recurring);
        assert_eq!(recurring.notes().map(String::as_str), Some("輪講"));
        assert_eq!(
            recurring.schedule(),
            &Schedule::Recurring {
                first_slot: first.time_period().clone(),
                rule: None
            }
        );
        assert!(
            recurring.one_time_slot().is_none(),
            "定期予約の予約枠は永続化層が決める"
        );
    }

    #[test]
    fn only_the_holder_can_cancel_a_reservation() {
        let reservation = Reservation::one_time(holder(), period(), vec![gpu()], None).unwrap();
        let other = EmailAddress::new("other@example.com".to_string()).unwrap();

        let refused = reservation.clone().cancel(&other).unwrap_err();
        let event = reservation.clone().cancel(&holder()).unwrap();

        assert!(matches!(refused, ReservationError::NotHolder { .. }));
        assert_eq!(event, ReservationEvent::Cancelled(reservation));
    }

    #[test]
    fn a_one_time_reservation_reads_back_from_its_slot() {
        let reservation = Reservation::one_time(holder(), period(), vec![gpu()], None).unwrap();

        let read_back = Reservation::from_one_time_slot(&reservation.one_time_slot().unwrap());

        assert_eq!(read_back, reservation);
    }
}
