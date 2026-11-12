//! リソースの空き算出
//!
//! 予約枠（`ReservedSlot`）は「誰がいつ使うか」を表すが、「いつ空いているか」は
//! そのままでは読み取れない。ここでは対象期間から予約枠の占める区間を差し引き、
//! リソースごとの空き区間を求める。
//!
//! # 空きの意味
//! 空きとは「その期間に重なる予約枠がない」ことである。予約されないまま使われている
//! リソースは予約枠として現れないため、ここでは空きとして扱われる
//! （`ReservedSlotRepository`および`ResourceConflictChecker`と同じ立場）。

use crate::domain::aggregates::reservation::ReservedSlot;
use crate::domain::aggregates::reservation::value_objects::{ReservedSlotId, Resource};
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use chrono::{DateTime, Utc};

/// あるリソースを押さえている予約枠の一区間
///
/// 対象期間の外にはみ出す予約枠は、期間内に収まる部分だけを持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusyPeriod {
    slot_id: ReservedSlotId,
    holder_email: EmailAddress,
    period: TimePeriod,
}

impl BusyPeriod {
    /// 押さえている予約枠のID
    pub fn slot_id(&self) -> &ReservedSlotId {
        &self.slot_id
    }

    /// 予約者のメールアドレス
    pub fn holder_email(&self) -> &EmailAddress {
        &self.holder_email
    }

    /// 対象期間内で押さえられている区間
    pub fn period(&self) -> &TimePeriod {
        &self.period
    }
}

/// ある瞬間におけるリソースの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvailabilityState<'a> {
    /// 空いている
    ///
    /// `until`は次に予約枠が始まる時刻。対象期間の終わりまで予約枠がなければ`None`。
    /// 「いつまで使えるか」は対象期間の取り方に左右されるため、期間の終端は
    /// 「まだ埋まっていない」を意味するに過ぎず、時刻としては伝えない。
    Free { until: Option<DateTime<Utc>> },
    /// 予約枠で埋まっている
    Busy(&'a BusyPeriod),
}

/// 対象期間における、あるリソースの空き
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAvailability {
    resource: Resource,
    window: TimePeriod,
    busy_periods: Vec<BusyPeriod>,
    free_periods: Vec<TimePeriod>,
}

impl ResourceAvailability {
    /// 対象のリソース
    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    /// 算出の対象とした期間
    pub fn window(&self) -> &TimePeriod {
        &self.window
    }

    /// 予約枠で埋まっている区間（開始順）
    ///
    /// 予約枠ごとに分かれている。同じリソースを続けて押さえる別々の予約枠は、
    /// 時間的に隣り合っていても1件にはまとめない（予約者が異なりうるため）。
    pub fn busy_periods(&self) -> &[BusyPeriod] {
        &self.busy_periods
    }

    /// 空いている区間（開始順）
    pub fn free_periods(&self) -> &[TimePeriod] {
        &self.free_periods
    }

    /// 対象期間の全体が空いているか
    pub fn is_entirely_free(&self) -> bool {
        self.busy_periods.is_empty()
    }

    /// 指定時刻に空いているか
    ///
    /// いつまで空いているか・誰が使っているかまで要るなら`state_at`を使う。
    pub fn is_free_at(&self, instant: DateTime<Utc>) -> bool {
        matches!(self.state_at(instant), AvailabilityState::Free { .. })
    }

    /// 指定時刻におけるリソースの状態
    ///
    /// 予約枠の終了時刻ちょうどは空きとして扱う（区間は終端を含まない）。
    pub fn state_at(&self, instant: DateTime<Utc>) -> AvailabilityState<'_> {
        let occupying = self
            .busy_periods
            .iter()
            .find(|busy| busy.period.start() <= instant && instant < busy.period.end());

        match occupying {
            Some(busy) => AvailabilityState::Busy(busy),
            None => AvailabilityState::Free {
                until: self
                    .busy_periods
                    .iter()
                    .map(|busy| busy.period.start())
                    .find(|start| *start > instant),
            },
        }
    }

    /// 指定時刻以降で最も早く空く時刻
    ///
    /// 指定時刻がすでに空き区間の中にあれば、その時刻自体を返す。
    /// 対象期間の終わりまで埋まっていれば`None`。
    pub fn next_free_at(&self, instant: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // 空き区間は開始順で互いに重ならないため、指定時刻より後に終わる最初の区間が
        // 指定時刻以降に使える最初の空きになる
        self.free_periods
            .iter()
            .find(|period| period.end() > instant)
            .map(|period| period.start().max(instant))
    }
}

/// 対象期間における各リソースの空きを算出する
///
/// # 引数
/// * `resources` - 対象のリソース（この並び順で返す）
/// * `window` - 算出の対象期間
/// * `slots` - `window`に重なる予約枠。呼び出し側がリポジトリから取得して渡す
///
/// # 戻り値
/// `resources`と同じ並びの空き情報
pub fn calculate(
    resources: &[Resource],
    window: &TimePeriod,
    slots: &[ReservedSlot],
) -> Vec<ResourceAvailability> {
    resources
        .iter()
        .map(|resource| calculate_one(resource, window, slots))
        .collect()
}

/// 1つのリソースについて空きを算出する
fn calculate_one(
    resource: &Resource,
    window: &TimePeriod,
    slots: &[ReservedSlot],
) -> ResourceAvailability {
    let mut busy_periods: Vec<BusyPeriod> = slots
        .iter()
        .filter(|slot| {
            slot.resources()
                .iter()
                .any(|reserved| reserved.conflicts_with(resource))
        })
        .filter_map(|slot| {
            clip_to(slot.time_period(), window).map(|period| BusyPeriod {
                slot_id: slot.id().clone(),
                holder_email: slot.holder_email().clone(),
                period,
            })
        })
        .collect();

    busy_periods.sort_by_key(|busy| busy.period.start());

    let free_periods = free_periods_between(window, &busy_periods);

    ResourceAvailability {
        resource: resource.clone(),
        window: window.clone(),
        busy_periods,
        free_periods,
    }
}

/// 期間を対象範囲の内側に切り詰める
///
/// 対象範囲と重ならない場合は`None`。
fn clip_to(period: &TimePeriod, window: &TimePeriod) -> Option<TimePeriod> {
    TimePeriod::new(
        period.start().max(window.start()),
        period.end().min(window.end()),
    )
    .ok()
}

/// 対象期間から、埋まっている区間を差し引いた残りを求める
///
/// `busy_periods`は開始順で、対象期間の内側に収まっていること。
/// 区間同士が重なっていても、隣り合っていても、残りは正しく求まる。
fn free_periods_between(window: &TimePeriod, busy_periods: &[BusyPeriod]) -> Vec<TimePeriod> {
    let mut free_periods = Vec::new();
    let mut cursor = window.start();

    for busy in busy_periods {
        if busy.period.start() > cursor
            && let Ok(period) = TimePeriod::new(cursor, busy.period.start())
        {
            free_periods.push(period);
        }
        cursor = cursor.max(busy.period.end());
    }

    if cursor < window.end()
        && let Ok(period) = TimePeriod::new(cursor, window.end())
    {
        free_periods.push(period);
    }

    free_periods
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use chrono::{Duration, TimeZone};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, hour, 0, 0).unwrap()
    }

    fn window() -> TimePeriod {
        TimePeriod::new(at(9), at(21)).unwrap()
    }

    fn gpu(device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            "gpu-server-1".to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))
    }

    fn slot(holder: &str, resources: Vec<Resource>, from: u32, to: u32) -> ReservedSlot {
        ReservedSlot::of_new_one_time_reservation(
            EmailAddress::new(holder.to_string()).unwrap(),
            TimePeriod::new(at(from), at(to)).unwrap(),
            resources,
            None,
        )
        .unwrap()
    }

    fn availability_of(resource: &Resource, slots: &[ReservedSlot]) -> ResourceAvailability {
        calculate(std::slice::from_ref(resource), &window(), slots)
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn a_resource_with_no_slots_is_free_for_the_whole_window() {
        let availability = availability_of(&gpu(0), &[]);

        assert!(availability.is_entirely_free());
        assert_eq!(availability.free_periods(), &[window()]);
    }

    #[test]
    fn a_slot_covering_the_window_leaves_no_free_time() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 9, 21)];

        let availability = availability_of(&gpu(0), &slots);

        assert!(
            availability.free_periods().is_empty(),
            "隙間なく埋まっているなら空きは1件も残らない: {:?}",
            availability.free_periods()
        );
    }

    #[test]
    fn the_time_before_and_after_a_slot_is_free() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 13, 17)];

        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.free_periods(),
            &[
                TimePeriod::new(at(9), at(13)).unwrap(),
                TimePeriod::new(at(17), at(21)).unwrap(),
            ]
        );
    }

    #[test]
    fn back_to_back_slots_do_not_leave_an_empty_gap_between_them() {
        let slots = vec![
            slot("a@example.com", vec![gpu(0)], 13, 15),
            slot("b@example.com", vec![gpu(0)], 15, 17),
        ];

        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.free_periods(),
            &[
                TimePeriod::new(at(9), at(13)).unwrap(),
                TimePeriod::new(at(17), at(21)).unwrap(),
            ],
            "終わりと始まりが同じ時刻なら、その間に長さ0の空きは生まれない"
        );
        assert_eq!(
            availability.busy_periods().len(),
            2,
            "予約者が異なるので、埋まっている側は1件にまとめない"
        );
    }

    #[test]
    fn overlapping_slots_do_not_confuse_the_remaining_free_time() {
        // 既存の予約枠同士が重複している状況（人手でカレンダーを編集すると起こりうる）
        let slots = vec![
            slot("a@example.com", vec![gpu(0)], 13, 18),
            slot("b@example.com", vec![gpu(0)], 15, 17),
        ];

        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.free_periods(),
            &[
                TimePeriod::new(at(9), at(13)).unwrap(),
                TimePeriod::new(at(18), at(21)).unwrap(),
            ],
            "内側に含まれる予約枠が、外側の予約枠の後ろに空きを作ってはいけない"
        );
    }

    #[test]
    fn a_slot_running_past_the_window_is_clipped_to_it() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 6, 12)];

        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.busy_periods()[0].period(),
            &TimePeriod::new(at(9), at(12)).unwrap(),
            "対象期間の外側は算出の対象にしない"
        );
    }

    #[test]
    fn slots_of_other_resources_are_ignored() {
        let slots = vec![slot("holder@example.com", vec![gpu(1)], 13, 17)];

        let availability = availability_of(&gpu(0), &slots);

        assert!(
            availability.is_entirely_free(),
            "別のGPUの予約枠で空きが削られてはいけない"
        );
    }

    #[test]
    fn a_slot_holding_several_resources_occupies_each_of_them() {
        let slots = vec![slot(
            "holder@example.com",
            vec![gpu(0), gpu(1), gpu(2)],
            13,
            17,
        )];

        let availabilities = calculate(&[gpu(0), gpu(1), gpu(2)], &window(), &slots);

        for availability in &availabilities {
            assert_eq!(
                availability.busy_periods().len(),
                1,
                "まとめて押さえられた3枚は、それぞれ埋まっている: {:?}",
                availability.resource()
            );
        }
    }

    #[test]
    fn the_state_during_a_slot_names_who_holds_it() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &slots);

        let AvailabilityState::Busy(busy) = availability.state_at(at(14)) else {
            panic!("予約枠の最中は使用中のはず");
        };

        assert_eq!(busy.holder_email().as_str(), "holder@example.com");
        assert_eq!(busy.period().end(), at(17));
    }

    #[test]
    fn the_state_when_free_tells_when_the_next_slot_starts() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &slots);

        let AvailabilityState::Free { until } = availability.state_at(at(10)) else {
            panic!("予約枠の前は空いているはず");
        };

        assert_eq!(
            until,
            Some(at(13)),
            "いつまで使えるかが分からなければ、使い始めてよいか判断できない"
        );
    }

    #[test]
    fn whether_a_resource_is_free_follows_the_slots() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &slots);

        assert!(availability.is_free_at(at(10)), "予約枠の前は空いている");
        assert!(
            !availability.is_free_at(at(14)),
            "予約枠の最中はふさがっている"
        );
        assert!(availability.is_free_at(at(18)), "予約枠の後は空いている");
    }

    #[test]
    fn the_state_at_the_end_of_a_slot_is_already_free() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &slots);

        assert!(
            matches!(
                availability.state_at(at(17)),
                AvailabilityState::Free { .. }
            ),
            "終了時刻ちょうどは次の人が使える"
        );
    }

    #[test]
    fn the_state_when_nothing_is_reserved_has_no_end_in_sight() {
        let availability = availability_of(&gpu(0), &[]);

        assert_eq!(
            availability.state_at(at(10)),
            AvailabilityState::Free { until: None },
            "対象期間の終わりは予約枠の予定ではないので、期限として伝えない"
        );
    }

    #[test]
    fn a_busy_resource_reports_when_it_frees_up() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 9, 17)];
        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(availability.next_free_at(at(10)), Some(at(17)));
    }

    #[test]
    fn a_free_resource_frees_up_right_now() {
        let availability = availability_of(&gpu(0), &[]);

        assert_eq!(
            availability.next_free_at(at(10)),
            Some(at(10)),
            "いま空いているなら、待つ必要はない"
        );
    }

    #[test]
    fn a_resource_free_between_two_slots_is_usable_right_now() {
        let slots = vec![
            slot("a@example.com", vec![gpu(0)], 13, 15),
            slot("b@example.com", vec![gpu(0)], 17, 19),
        ];
        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.next_free_at(at(10)),
            Some(at(10)),
            "先に予約枠が控えていても、いま空いているなら待つ必要はない"
        );
    }

    #[test]
    fn a_busy_resource_reports_the_first_gap_not_a_later_one() {
        let slots = vec![
            slot("a@example.com", vec![gpu(0)], 9, 13),
            slot("b@example.com", vec![gpu(0)], 15, 17),
        ];
        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.next_free_at(at(10)),
            Some(at(13)),
            "予約枠の合間も使える時間なので、その後の空きを答えてはいけない"
        );
    }

    #[test]
    fn a_resource_booked_to_the_end_of_the_window_never_frees_up() {
        let slots = vec![slot("holder@example.com", vec![gpu(0)], 9, 21)];
        let availability = availability_of(&gpu(0), &slots);

        assert_eq!(
            availability.next_free_at(at(10)),
            None,
            "見ている範囲では空かない"
        );
    }

    #[test]
    fn availabilities_come_back_in_the_order_the_resources_were_given() {
        let resources = vec![gpu(3), gpu(0), gpu(2)];

        let availabilities = calculate(&resources, &window(), &[]);

        let returned: Vec<&Resource> = availabilities
            .iter()
            .map(ResourceAvailability::resource)
            .collect();
        assert_eq!(returned, vec![&gpu(3), &gpu(0), &gpu(2)]);
    }

    #[test]
    fn a_slot_that_ends_before_the_window_does_not_appear() {
        let window = TimePeriod::new(at(9), at(21)).unwrap();
        let past = ReservedSlot::of_new_one_time_reservation(
            EmailAddress::new("holder@example.com".to_string()).unwrap(),
            TimePeriod::new(at(9) - Duration::hours(3), at(9)).unwrap(),
            vec![gpu(0)],
            None,
        )
        .unwrap();

        let availability = calculate(&[gpu(0)], &window, &[past])
            .into_iter()
            .next()
            .unwrap();

        assert!(
            availability.is_entirely_free(),
            "対象期間の直前に終わった予約枠は、いまの空きに関係しない"
        );
    }
}
