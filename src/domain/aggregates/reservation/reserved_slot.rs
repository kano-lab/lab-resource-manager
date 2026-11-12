use super::value_objects::OccurrenceKey;
use crate::domain::common::TimePeriod;

/// 予約枠（予約の中の回）
///
/// 予約が実際に押さえている、ひとつながりの時間。予約が自分の日程から求めるもので、
/// 単独では作れず、保存もされない（[`super::Reservation::slot`]）。
/// 予約者とリソースは持たず、属する予約のものを使う。予約の外から指すときは
/// 予約枠ID（予約ID ＋ 回の鍵）を使う。
#[derive(Debug, Clone, PartialEq)]
pub struct ReservedSlot {
    key: OccurrenceKey,
    time_period: TimePeriod,
    notes: Option<String>,
}

impl ReservedSlot {
    pub(super) fn new(key: OccurrenceKey, time_period: TimePeriod, notes: Option<String>) -> Self {
        Self {
            key,
            time_period,
            notes,
        }
    }

    /// 予約の中で回を区別する鍵
    pub fn key(&self) -> &OccurrenceKey {
        &self.key
    }

    /// 期間を取得
    pub fn time_period(&self) -> &TimePeriod {
        &self.time_period
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }
}
