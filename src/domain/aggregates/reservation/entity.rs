use super::errors::ReservationError;
use super::events::ReservationEvent;
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

    /// 版を除いて同じ内容か
    ///
    /// 保存先で版だけが変わることがある（内容を変えずに書き直されたなど）。
    /// 予約に何かが起きたかを見るときは、版を比べない。
    pub fn has_same_content_as(&self, other: &Reservation) -> bool {
        self.id == other.id
            && self.holder_email == other.holder_email
            && self.resources == other.resources
            && self.notes == other.notes
            && self.schedule == other.schedule
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
        }
    }

    /// 期間に重なる予約枠を、開始順に求める
    pub fn slots_in(&self, window: &TimePeriod) -> Vec<ReservedSlot> {
        match &self.schedule {
            Schedule::OneTime(_) => self
                .slot(&OccurrenceKey::Only)
                .filter(|slot| slot.time_period().overlaps_with(window))
                .into_iter()
                .collect(),
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
        self.require_slot(key)?;
        match &mut self.schedule {
            Schedule::OneTime(current) => *current = time_period,
        }
        Ok(ReservationEvent::Changed(self.clone()))
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
        self.require_slot(key)?;
        match &self.schedule {
            Schedule::OneTime(_) => self.notes = Some(notes),
        }
        Ok(ReservationEvent::Changed(self.clone()))
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
        match &mut self.schedule {
            Schedule::OneTime(current) => *current = shortened,
        }
        Ok(ReservationEvent::EndedEarly(self.clone()))
    }

    /// 予約枠を取り消す
    ///
    /// 単発予約の予約枠を取り消すと、その予約も取り消し済みになる。
    ///
    /// # Errors
    /// `reschedule_slot`と同じ
    pub fn cancel_slot(
        &mut self,
        key: &OccurrenceKey,
        by: &EmailAddress,
    ) -> Result<ReservationEvent, ReservationError> {
        self.ensure_operable(by)?;
        self.require_slot(key)?;
        match &self.schedule {
            Schedule::OneTime(_) => self.cancel(by),
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
