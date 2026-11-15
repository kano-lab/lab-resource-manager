use super::reserved_slot::ReservedSlot;
use super::value_objects::{OccurrenceKey, RecurrenceRule, Version};
use crate::domain::common::TimePeriod;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use std::collections::{BTreeMap, BTreeSet};

/// 定期予約の日程
///
/// 最初の予約枠と繰り返しの規則から、回ごとの予約枠が決まる。回の時刻は規則の時間帯で
/// 数える（「毎週月曜10時」は、その時間帯の10時）。時刻の長さは最初の予約枠と同じ。
///
/// 規則どおりでない回（時間や備考を変えた回、取り消した回）は、例外として回の本来の
/// 開始時刻ごとに持つ。例外は回ごとに保存先での版を持つ。
///
/// 読んだあとに変えた回も覚えておく。保存するときに書くのはその回だけで、ほかの回を
/// 書き戻すことはしない。
#[derive(Debug, Clone, PartialEq)]
pub struct RecurringSchedule {
    first: TimePeriod,
    rule: RecurrenceRule,
    time_zone: Tz,
    exceptions: BTreeMap<DateTime<Utc>, Exception>,
    unsaved: BTreeSet<DateTime<Utc>>,
}

/// 規則どおりでない回
#[derive(Debug, Clone, PartialEq)]
pub struct Exception {
    change: ExceptionChange,
    version: Option<Version>,
}

/// 回が規則からどう外れたか
#[derive(Debug, Clone, PartialEq)]
pub enum ExceptionChange {
    /// 時間や備考を変えた回
    Changed {
        /// この回の期間
        time_period: TimePeriod,
        /// この回の備考
        notes: Option<String>,
    },
    /// 取り消した回
    Cancelled,
}

impl Exception {
    /// 例外を組み立てる
    ///
    /// `version`は保存先でのこの回の版で、まだ保存していなければ`None`。
    pub fn new(change: ExceptionChange, version: Option<Version>) -> Self {
        Self { change, version }
    }

    /// 回が規則からどう外れたか
    pub fn change(&self) -> &ExceptionChange {
        &self.change
    }

    /// 保存先でのこの回の版
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}

impl RecurringSchedule {
    /// 定期予約の日程を組み立てる
    ///
    /// # Arguments
    /// * `first` - 最初の予約枠の期間
    /// * `rule` - 繰り返しの規則
    /// * `time_zone` - 繰り返しを数える時間帯
    /// * `exceptions` - 規則どおりでない回（回の本来の開始時刻ごと）
    pub fn new(
        first: TimePeriod,
        rule: RecurrenceRule,
        time_zone: Tz,
        exceptions: BTreeMap<DateTime<Utc>, Exception>,
    ) -> Self {
        Self {
            first,
            rule,
            time_zone,
            exceptions,
            unsaved: BTreeSet::new(),
        }
    }

    /// 最初の予約枠の期間
    pub fn first(&self) -> &TimePeriod {
        &self.first
    }

    /// 繰り返しの規則
    pub fn rule(&self) -> &RecurrenceRule {
        &self.rule
    }

    /// 繰り返しを数える時間帯
    pub fn time_zone(&self) -> Tz {
        self.time_zone
    }

    /// 規則どおりでない回（回の本来の開始時刻ごと）
    pub fn exceptions(&self) -> &BTreeMap<DateTime<Utc>, Exception> {
        &self.exceptions
    }

    /// 読んだあとに変えて、まだ保存していない回（本来の開始時刻）
    pub fn unsaved_occurrences(&self) -> &BTreeSet<DateTime<Utc>> {
        &self.unsaved
    }

    /// 版と保存の有無を除いて同じ内容か
    pub fn has_same_content_as(&self, other: &RecurringSchedule) -> bool {
        self.first == other.first
            && self.rule == other.rule
            && self.time_zone == other.time_zone
            && self.exceptions.len() == other.exceptions.len()
            && self
                .exceptions
                .iter()
                .zip(&other.exceptions)
                .all(|((a_start, a), (b_start, b))| a_start == b_start && a.change == b.change)
    }

    /// 1回の長さ
    fn duration(&self) -> Duration {
        self.first.end() - self.first.start()
    }

    /// `start`に本来始まる回があるか
    fn has_occurrence_at(&self, start: DateTime<Utc>) -> bool {
        self.rule
            .has_occurrence_at(self.first.start(), self.time_zone, start)
    }

    /// 本来`start`に始まる回の予約枠（取り消した回や、規則に無い回なら`None`）
    pub(super) fn slot(
        &self,
        start: DateTime<Utc>,
        series_notes: Option<&String>,
    ) -> Option<ReservedSlot> {
        if !self.has_occurrence_at(start) {
            return None;
        }
        self.slot_of(start, series_notes)
    }

    /// 回の予約枠。規則に回があることは呼び出し側が確かめている
    fn slot_of(&self, start: DateTime<Utc>, series_notes: Option<&String>) -> Option<ReservedSlot> {
        let key = OccurrenceKey::At(start);
        match self.exceptions.get(&start).map(Exception::change) {
            Some(ExceptionChange::Cancelled) => None,
            Some(ExceptionChange::Changed { time_period, notes }) => {
                Some(ReservedSlot::new(key, time_period.clone(), notes.clone()))
            }
            None => TimePeriod::new(start, start + self.duration())
                .ok()
                .map(|time_period| ReservedSlot::new(key, time_period, series_notes.cloned())),
        }
    }

    /// 期間に重なる予約枠を、開始順に求める
    pub(super) fn slots_in(
        &self,
        window: &TimePeriod,
        series_notes: Option<&String>,
    ) -> Vec<ReservedSlot> {
        // 窓より前に始まっても、窓にかかる回がある
        let from = window.start() - self.duration();
        let mut slots: Vec<ReservedSlot> = self
            .rule
            .occurrence_starts(self.first.start(), self.time_zone, from, window.end())
            .into_iter()
            .filter(|start| !self.exceptions.contains_key(start))
            .filter_map(|start| self.slot_of(start, series_notes))
            .collect();
        // 時間を変えた回は、本来の時刻が窓の外でも、変えた先が窓にかかることがある
        slots.extend(
            self.exceptions
                .keys()
                .filter(|start| self.has_occurrence_at(**start))
                .filter_map(|start| self.slot_of(*start, series_notes)),
        );
        slots.retain(|slot| slot.time_period().overlaps_with(window));
        slots.sort_by_key(|slot| slot.time_period().start());
        slots
    }

    /// 最後の予約枠が終わる時刻（繰り返しに終わりが無ければ`None`）
    pub(super) fn last_end(&self) -> Option<DateTime<Utc>> {
        let last_start = self.rule.last_start(self.first.start(), self.time_zone)?;
        let regular_end = last_start + self.duration();
        let changed_end = self
            .exceptions
            .values()
            .filter_map(|exception| match &exception.change {
                ExceptionChange::Changed { time_period, .. } => Some(time_period.end()),
                ExceptionChange::Cancelled => None,
            })
            .max();
        Some(changed_end.map_or(regular_end, |end| end.max(regular_end)))
    }

    /// 回を規則から外す（すでに外れていれば、外れ方を置き換える）
    ///
    /// 保存先での版は、読んだときのものを残す。書き戻すときに、その回が読んだあとに
    /// 変わっていないかを確かめるのに使う。
    pub(super) fn set_exception(&mut self, start: DateTime<Utc>, change: ExceptionChange) {
        let version = self
            .exceptions
            .get(&start)
            .and_then(|exception| exception.version.clone());
        self.exceptions
            .insert(start, Exception::new(change, version));
        self.unsaved.insert(start);
    }

    /// 回を保存したあとの、保存先での版を記録する
    pub(super) fn record_exception_version(
        &mut self,
        start: DateTime<Utc>,
        version: Option<Version>,
    ) {
        if let Some(exception) = self.exceptions.get_mut(&start) {
            exception.version = version;
        }
        self.unsaved.remove(&start);
    }
}
