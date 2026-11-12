use super::errors::ReservationError;
use super::events::ReservationEvent;
use super::recurring::{ExceptionChange, RecurringSchedule};
use super::reserved_slot::ReservedSlot;
use super::value_objects::{OccurrenceKey, ReservationId, ReservedSlotId, Resource, Version};
use crate::domain::common::{EmailAddress, TimePeriod};
use chrono::{DateTime, Utc};

/// 予約（集約の根）
///
/// 予約者が、リソースを使う時間を前もって押さえる約束。作る・取り消すの単位である。
/// 予約者・リソース・備考はどの予約にもあり、いつ押さえるかは日程（[`Schedule`]）が表す。
///
/// 予約が実際に押さえている時間である予約枠（[`ReservedSlot`]）は、予約の中の回である。
/// 予約は予約枠をフィールドとして持たず、日程から求める（[`Self::slot`]、[`Self::slots_in`]）。
/// 予約枠への操作（時間の変更・今で終了・取り消し）もすべて予約を通す。決まりは予約が
/// 一か所で守る。
#[derive(Debug, Clone, PartialEq)]
pub struct Reservation {
    id: ReservationId,
    holder_email: EmailAddress,
    resources: Vec<Resource>,
    notes: Option<String>,
    schedule: Schedule,
    status: ReservationStatus,
    version: Option<Version>,
}

/// 予約の日程（いつ時間を押さえるか）
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    /// 単発: 期間が1つに決まっている
    ///
    /// 単発予約は予約枠を1つだけ持ち、その予約枠は予約と同じIDで識別される。
    OneTime(TimePeriod),
    /// 定期: 最初の予約枠と繰り返しの規則から、回ごとの予約枠が決まる
    ///
    /// 予約枠は回の本来の開始時刻（[`OccurrenceKey::At`]）で識別される。
    Recurring(RecurringSchedule),
}

impl Schedule {
    /// 版を除いて同じ内容か
    fn has_same_content_as(&self, other: &Schedule) -> bool {
        match (self, other) {
            (Schedule::OneTime(a), Schedule::OneTime(b)) => a == b,
            (Schedule::Recurring(a), Schedule::Recurring(b)) => a.has_same_content_as(b),
            _ => false,
        }
    }
}

/// 予約の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationStatus {
    /// 有効
    Active,
    /// 取り消し済み。どの操作も受け付けず、保存すると保存先から消える
    Cancelled,
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
            None,
        )
    }

    /// 保存先から読んだ予約を組み立てる（リポジトリからの再構築用）
    ///
    /// 保存先にある予約は取り消されていない。`version`は保存先での版で、
    /// まだ保存していない予約なら`None`。
    ///
    /// # Errors
    /// リソースが空の場合、`ReservationError::NoResourceItems`を返す
    pub fn reconstruct(
        id: ReservationId,
        holder_email: EmailAddress,
        resources: Vec<Resource>,
        notes: Option<String>,
        schedule: Schedule,
        version: Option<Version>,
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
            status: ReservationStatus::Active,
            version,
        })
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

    /// 定期予約か
    pub fn is_recurring(&self) -> bool {
        matches!(self.schedule, Schedule::Recurring(_))
    }

    /// 状態を取得
    pub fn status(&self) -> ReservationStatus {
        self.status
    }

    /// 保存先での版を取得（まだ保存していなければ`None`）
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// 保存したあとの版を記録する
    ///
    /// リポジトリの実装が、保存先へ書いたあとに呼ぶ。
    pub fn record_version(&mut self, version: Option<Version>) {
        self.version = version;
    }

    /// 定期予約の回を保存したあとの、その回の版を記録する
    ///
    /// 規則どおりでない回（例外）は、回ごとに保存先での版を持つ。リポジトリの実装が、
    /// その回を保存先へ書いたあとに呼ぶ。
    pub fn record_occurrence_version(&mut self, start: DateTime<Utc>, version: Option<Version>) {
        if let Schedule::Recurring(recurring) = &mut self.schedule {
            recurring.record_exception_version(start, version);
        }
    }

    /// 版を除いて同じ内容か
    ///
    /// 保存先で版だけが変わることがある（内容を変えずに書き直されたなど）。
    /// 予約に何かが起きたかを見るときは、版を比べない。
    pub fn has_same_content_as(&self, other: &Reservation) -> bool {
        self.id == other.id
            && self.holder_email == other.holder_email
            && self.resources == other.resources
            && self.notes == other.notes
            && self.schedule.has_same_content_as(&other.schedule)
            && self.status == other.status
    }

    /// この予約の回を指す予約枠ID
    pub fn slot_id(&self, key: OccurrenceKey) -> ReservedSlotId {
        ReservedSlotId::new(self.id.clone(), key)
    }

    /// 鍵で指した予約枠を求める
    ///
    /// 取り消し済みの予約や、日程に無い回なら`None`を返す。
    pub fn slot(&self, key: &OccurrenceKey) -> Option<ReservedSlot> {
        if self.status == ReservationStatus::Cancelled {
            return None;
        }
        match (&self.schedule, key) {
            (Schedule::OneTime(time_period), OccurrenceKey::Only) => Some(ReservedSlot::new(
                OccurrenceKey::Only,
                time_period.clone(),
                self.notes.clone(),
            )),
            (Schedule::Recurring(recurring), OccurrenceKey::At(start)) => {
                recurring.slot(*start, self.notes.as_ref())
            }
            _ => None,
        }
    }

    /// 期間に重なる予約枠を、開始順に求める
    pub fn slots_in(&self, window: &TimePeriod) -> Vec<ReservedSlot> {
        if self.status == ReservationStatus::Cancelled {
            return Vec::new();
        }
        match &self.schedule {
            Schedule::OneTime(_) => self
                .slot(&OccurrenceKey::Only)
                .filter(|slot| slot.time_period().overlaps_with(window))
                .into_iter()
                .collect(),
            Schedule::Recurring(recurring) => recurring.slots_in(window, self.notes.as_ref()),
        }
    }

    /// 最後の予約枠が終わる時刻（定期予約の繰り返しに終わりが無ければ`None`）
    pub fn last_end(&self) -> Option<DateTime<Utc>> {
        match &self.schedule {
            Schedule::OneTime(time_period) => Some(time_period.end()),
            Schedule::Recurring(recurring) => recurring.last_end(),
        }
    }

    /// 予約枠の時間を変える
    ///
    /// 他の予約の予約枠と重ならないかは、ここでは確かめない。それには他の予約を
    /// 読む必要があるため、呼び出し側が競合の判定（`ResourceConflictChecker`）を行う。
    ///
    /// # Errors
    /// - 予約者でない場合、`ReservationError::NotHolder`
    /// - 取り消し済みの場合、`ReservationError::AlreadyCancelled`
    /// - 予約枠が無い場合、`ReservationError::NoSuchSlot`
    pub fn reschedule_slot(
        &mut self,
        key: &OccurrenceKey,
        time_period: TimePeriod,
        by: &EmailAddress,
    ) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        let slot = self.require_slot(key)?;
        let notes = slot.notes().cloned();
        Ok(self.replace_slot(&slot, time_period, notes, false))
    }

    /// 予約枠の備考を書き換える
    ///
    /// # Errors
    /// `reschedule_slot`と同じ
    pub fn change_slot_notes(
        &mut self,
        key: &OccurrenceKey,
        notes: String,
        by: &EmailAddress,
    ) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        let slot = self.require_slot(key)?;
        let time_period = slot.time_period().clone();
        Ok(self.replace_slot(&slot, time_period, Some(notes), false))
    }

    /// 進行中の予約枠を`at`の時点で締める（今で終了）
    ///
    /// 取り消しが予約枠そのものを無かったことにするのに対し、今で終了は
    /// 開始から`at`までを使った事実として残し、残りの時間だけを空きに戻す。
    ///
    /// # Errors
    /// - `reschedule_slot`と同じもの
    /// - `at`が開始時刻以前の場合、`ReservationError::NotYetStarted`
    /// - `at`が終了時刻以降の場合、`ReservationError::AlreadyEnded`
    pub fn end_slot_early(
        &mut self,
        key: &OccurrenceKey,
        at: DateTime<Utc>,
        by: &EmailAddress,
    ) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        let slot = self.require_slot(key)?;
        let shortened = Self::shorten(slot.time_period(), at)?;
        let notes = slot.notes().cloned();
        Ok(self.replace_slot(&slot, shortened, notes, true))
    }

    /// 予約枠を取り消す
    ///
    /// 単発予約の予約枠を取り消すと、その予約も取り消し済みになる。定期予約の予約枠を
    /// 取り消しても、予約とほかの予約枠は残る。
    ///
    /// # Errors
    /// `reschedule_slot`と同じ
    pub fn cancel_slot(
        &mut self,
        key: &OccurrenceKey,
        by: &EmailAddress,
    ) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        let slot = self.require_slot(key)?;
        match (&mut self.schedule, key) {
            (Schedule::Recurring(recurring), OccurrenceKey::At(start)) => {
                recurring.set_exception(*start, ExceptionChange::Cancelled);
                Ok(ReservationEvent::SlotCancelled {
                    reservation: self.clone(),
                    slot,
                })
            }
            _ => self.cancel(by),
        }
    }

    /// 予約を取り消す
    ///
    /// 取り消し済みの状態にする。保存すると保存先から消える。
    ///
    /// # Errors
    /// - 予約者でない場合、`ReservationError::NotHolder`
    /// - 取り消し済みの場合、`ReservationError::AlreadyCancelled`
    pub fn cancel(&mut self, by: &EmailAddress) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        self.status = ReservationStatus::Cancelled;
        Ok(ReservationEvent::Cancelled(self.clone()))
    }

    /// 予約者本人が、取り消されていない予約を操作しようとしているか確かめる
    fn ensure_operable(&self, by: &EmailAddress) -> Result<(), ReservationError> {
        if by != &self.holder_email {
            return Err(ReservationError::NotHolder { actor: by.clone() });
        }
        if self.status == ReservationStatus::Cancelled {
            return Err(ReservationError::AlreadyCancelled);
        }
        Ok(())
    }

    /// 予約枠の期間と備考を置き換え、起きた出来事を返す
    ///
    /// 単発予約は予約の日程と備考を書き換える。定期予約は、その回を規則どおりでない回
    /// （例外）にし、予約のほかの回には触れない。
    fn replace_slot(
        &mut self,
        slot: &ReservedSlot,
        time_period: TimePeriod,
        notes: Option<String>,
        ended_early: bool,
    ) -> ReservationEvent {
        let replaced = ReservedSlot::new(slot.key().clone(), time_period.clone(), notes.clone());
        match (&mut self.schedule, slot.key()) {
            (Schedule::Recurring(recurring), OccurrenceKey::At(start)) => {
                recurring.set_exception(*start, ExceptionChange::Changed { time_period, notes });
                let reservation = self.clone();
                if ended_early {
                    ReservationEvent::SlotEndedEarly {
                        reservation,
                        slot: replaced,
                    }
                } else {
                    ReservationEvent::SlotChanged {
                        reservation,
                        slot: replaced,
                    }
                }
            }
            (schedule, _) => {
                if let Schedule::OneTime(current) = schedule {
                    *current = time_period;
                }
                self.notes = notes;
                if ended_early {
                    ReservationEvent::EndedEarly(self.clone())
                } else {
                    ReservationEvent::Changed(self.clone())
                }
            }
        }
    }

    fn require_slot(&self, key: &OccurrenceKey) -> Result<ReservedSlot, ReservationError> {
        self.slot(key)
            .ok_or_else(|| ReservationError::NoSuchSlot(key.clone()))
    }

    /// 進行中の期間を`at`で締めた期間
    fn shorten(
        time_period: &TimePeriod,
        at: DateTime<Utc>,
    ) -> Result<TimePeriod, ReservationError> {
        if at <= time_period.start() {
            return Err(ReservationError::NotYetStarted {
                start: time_period.start(),
                at,
            });
        }
        if at >= time_period.end() {
            return Err(ReservationError::AlreadyEnded {
                end: time_period.end(),
                at,
            });
        }
        Ok(TimePeriod::new(time_period.start(), at)?)
    }
}

#[cfg(test)]
#[path = "entity_tests.rs"]
mod tests;
