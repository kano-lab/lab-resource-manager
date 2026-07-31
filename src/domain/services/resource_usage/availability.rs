//! リソースの空き算出
//!
//! 予約（`ResourceUsage`）は「誰がいつ使うか」を表すが、「いつ空いているか」は
//! そのままでは読み取れない。ここでは対象期間から予約の占める区間を差し引き、
//! リソースごとの空き区間を求める。
//!
//! # 空きの意味
//! 空きとは「その期間に重なる予約がない」ことである。予約されないまま使われている
//! リソースは予約として現れないため、ここでは空きとして扱われる
//! （`ResourceUsageRepository`および`ResourceConflictChecker`と同じ立場）。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::common::EmailAddress;
use chrono::{DateTime, Utc};

/// あるリソースを押さえている予約の一区間
///
/// 対象期間の外にはみ出す予約は、期間内に収まる部分だけを持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusyPeriod {
    usage_id: UsageId,
    owner_email: EmailAddress,
    period: TimePeriod,
}

impl BusyPeriod {
    /// 押さえている予約のID
    pub fn usage_id(&self) -> &UsageId {
        &self.usage_id
    }

    /// 予約者のメールアドレス
    pub fn owner_email(&self) -> &EmailAddress {
        &self.owner_email
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
    /// `until`は次に予約が始まる時刻。対象期間の終わりまで予約がなければ`None`。
    /// 「いつまで使えるか」は対象期間の取り方に左右されるため、期間の終端は
    /// 「まだ埋まっていない」を意味するに過ぎず、時刻としては伝えない。
    Free { until: Option<DateTime<Utc>> },
    /// 予約で埋まっている
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

    /// 予約で埋まっている区間（開始順）
    ///
    /// 予約ごとに分かれている。同じリソースを続けて押さえる別々の予約は、
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

    /// 指定時刻におけるリソースの状態
    ///
    /// 予約の終了時刻ちょうどは空きとして扱う（区間は終端を含まない）。
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
    /// 対象期間の終わりまで埋まっていれば`None`。
    pub fn next_free_at(&self, instant: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.free_periods
            .iter()
            .map(TimePeriod::start)
            .find(|start| *start >= instant)
            .or_else(|| {
                // 指定時刻が空き区間の途中にあれば、その時刻自体が答えになる
                self.free_periods
                    .iter()
                    .find(|period| period.start() <= instant && instant < period.end())
                    .map(|_| instant)
            })
    }
}

/// 対象期間における各リソースの空きを算出する
///
/// # 引数
/// * `resources` - 対象のリソース（この並び順で返す）
/// * `window` - 算出の対象期間
/// * `usages` - `window`に重なる予約。呼び出し側がリポジトリから取得して渡す
///
/// # 戻り値
/// `resources`と同じ並びの空き情報
pub fn calculate(
    resources: &[Resource],
    window: &TimePeriod,
    usages: &[ResourceUsage],
) -> Vec<ResourceAvailability> {
    resources
        .iter()
        .map(|resource| calculate_one(resource, window, usages))
        .collect()
}

/// 1つのリソースについて空きを算出する
fn calculate_one(
    resource: &Resource,
    window: &TimePeriod,
    usages: &[ResourceUsage],
) -> ResourceAvailability {
    let mut busy_periods: Vec<BusyPeriod> = usages
        .iter()
        .filter(|usage| {
            usage
                .resources()
                .iter()
                .any(|reserved| reserved.conflicts_with(resource))
        })
        .filter_map(|usage| {
            clip_to(usage.time_period(), window).map(|period| BusyPeriod {
                usage_id: usage.id().clone(),
                owner_email: usage.owner_email().clone(),
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
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
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

    fn reservation(owner: &str, resources: Vec<Resource>, from: u32, to: u32) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new(owner.to_string()).unwrap(),
            TimePeriod::new(at(from), at(to)).unwrap(),
            resources,
            None,
        )
        .unwrap()
    }

    fn availability_of(resource: &Resource, usages: &[ResourceUsage]) -> ResourceAvailability {
        calculate(std::slice::from_ref(resource), &window(), usages)
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn a_resource_with_no_reservations_is_free_for_the_whole_window() {
        let availability = availability_of(&gpu(0), &[]);

        assert!(availability.is_entirely_free());
        assert_eq!(availability.free_periods(), &[window()]);
    }

    #[test]
    fn a_reservation_covering_the_window_leaves_no_free_time() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 9, 21)];

        let availability = availability_of(&gpu(0), &usages);

        assert!(
            availability.free_periods().is_empty(),
            "隙間なく埋まっているなら空きは1件も残らない: {:?}",
            availability.free_periods()
        );
    }

    #[test]
    fn the_time_before_and_after_a_reservation_is_free() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 13, 17)];

        let availability = availability_of(&gpu(0), &usages);

        assert_eq!(
            availability.free_periods(),
            &[
                TimePeriod::new(at(9), at(13)).unwrap(),
                TimePeriod::new(at(17), at(21)).unwrap(),
            ]
        );
    }

    #[test]
    fn back_to_back_reservations_do_not_leave_an_empty_gap_between_them() {
        let usages = vec![
            reservation("a@example.com", vec![gpu(0)], 13, 15),
            reservation("b@example.com", vec![gpu(0)], 15, 17),
        ];

        let availability = availability_of(&gpu(0), &usages);

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
    fn overlapping_reservations_do_not_confuse_the_remaining_free_time() {
        // 既存の予約同士が重複している状況（人手でカレンダーを編集すると起こりうる）
        let usages = vec![
            reservation("a@example.com", vec![gpu(0)], 13, 18),
            reservation("b@example.com", vec![gpu(0)], 15, 17),
        ];

        let availability = availability_of(&gpu(0), &usages);

        assert_eq!(
            availability.free_periods(),
            &[
                TimePeriod::new(at(9), at(13)).unwrap(),
                TimePeriod::new(at(18), at(21)).unwrap(),
            ],
            "内側に含まれる予約が、外側の予約の後ろに空きを作ってはいけない"
        );
    }

    #[test]
    fn a_reservation_running_past_the_window_is_clipped_to_it() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 6, 12)];

        let availability = availability_of(&gpu(0), &usages);

        assert_eq!(
            availability.busy_periods()[0].period(),
            &TimePeriod::new(at(9), at(12)).unwrap(),
            "対象期間の外側は算出の対象にしない"
        );
    }

    #[test]
    fn reservations_of_other_resources_are_ignored() {
        let usages = vec![reservation("owner@example.com", vec![gpu(1)], 13, 17)];

        let availability = availability_of(&gpu(0), &usages);

        assert!(
            availability.is_entirely_free(),
            "別のGPUの予約で空きが削られてはいけない"
        );
    }

    #[test]
    fn a_reservation_holding_several_resources_occupies_each_of_them() {
        let usages = vec![reservation(
            "owner@example.com",
            vec![gpu(0), gpu(1), gpu(2)],
            13,
            17,
        )];

        let availabilities = calculate(&[gpu(0), gpu(1), gpu(2)], &window(), &usages);

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
    fn the_state_during_a_reservation_names_who_holds_it() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &usages);

        let AvailabilityState::Busy(busy) = availability.state_at(at(14)) else {
            panic!("予約の最中は使用中のはず");
        };

        assert_eq!(busy.owner_email().as_str(), "owner@example.com");
        assert_eq!(busy.period().end(), at(17));
    }

    #[test]
    fn the_state_when_free_tells_when_the_next_reservation_starts() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &usages);

        let AvailabilityState::Free { until } = availability.state_at(at(10)) else {
            panic!("予約の前は空いているはず");
        };

        assert_eq!(
            until,
            Some(at(13)),
            "いつまで使えるかが分からなければ、使い始めてよいか判断できない"
        );
    }

    #[test]
    fn the_state_at_the_end_of_a_reservation_is_already_free() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 13, 17)];
        let availability = availability_of(&gpu(0), &usages);

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
            "対象期間の終わりは予約の予定ではないので、期限として伝えない"
        );
    }

    #[test]
    fn a_busy_resource_reports_when_it_frees_up() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 9, 17)];
        let availability = availability_of(&gpu(0), &usages);

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
    fn a_resource_booked_to_the_end_of_the_window_never_frees_up() {
        let usages = vec![reservation("owner@example.com", vec![gpu(0)], 9, 21)];
        let availability = availability_of(&gpu(0), &usages);

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
    fn a_reservation_that_ends_before_the_window_does_not_appear() {
        let window = TimePeriod::new(at(9), at(21)).unwrap();
        let past = ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
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
            "対象期間の直前に終わった予約は、いまの空きに関係しない"
        );
    }
}
