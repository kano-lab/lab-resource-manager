//! 予約の変化から、予約に起きた出来事を読み取る

use crate::domain::aggregates::reservation::{
    Exception, OccurrenceKey, RecurringSchedule, Reservation, ReservationEvent, ReservationId,
    Schedule,
};
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use chrono::{DateTime, Utc};
use std::collections::{BTreeSet, HashMap};

/// 予約の変化から、予約に起きた出来事を読み取るドメインサービス
///
/// 監視している期間の予約を前回と今回で突き合わせると、現れた・変わった・
/// 見えなくなったという差分が得られる。差分そのものは、予約に何が起きたかを決めない。
/// 予約は集約のまま突き合わせる。
///
/// - 予約が現れた → 予約された
/// - 予約の内容が変わった → 予約が変わった（単発予約の予約枠が、終わりだけ今より前へ
///   縮んでいれば、今で終了した）。定期予約で、予約そのものは変わらず一部の回だけが
///   変わったなら、回ごとに、その予約枠が変わった・今で終了した・取り消された
/// - 予約が予定より早く見えなくなった → 取り消されたか、今で終了したか、監視している
///   期間の外へ動かされた
///
/// 最後のものを見分けるのは予約の知識である。今で終了した予約は、終了時刻が
/// 縮んで監視範囲から外れるだけで、使った分が予約枠として残っている。取り消された
/// 予約は残っていない。そこで、見えなくなった予約を改めてIDで引いて確かめる。
///
/// 予定どおり終わった予約も見えなくなるが、何も起きていないので出来事にしない。
/// 版だけが変わった予約や、内容を変えずに監視している期間から外れただけの予約も、
/// 出来事にしない。
#[derive(Debug, Clone, Default)]
pub struct ReservationChangeInterpreter;

/// 読み取った結果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterpretedChanges {
    /// 予約に起きた出来事
    pub events: Vec<ReservationEvent>,
    /// 新しく現れた予約の数
    pub appeared: usize,
    /// 予定より早く見えなくなった予約の数
    pub disappeared: usize,
    /// 見えなくなったが、まだ続いている予約（読み直した姿）
    ///
    /// 監視している期間に予約枠が無くなっただけの予約である（回の間隔が監視期間より長い
    /// 定期予約や、監視期間の外へ動かした予約）。呼び出し側は次回の「前回見えていた予約」に
    /// これを含める。含めないと、予約枠が監視期間に戻ってきたときに新しい予約と読んでしまう。
    pub carried_over: Vec<Reservation>,
}

impl ReservationChangeInterpreter {
    /// 新しいインスタンスを作成
    pub fn new() -> Self {
        Self
    }

    /// 前回と今回の予約から、予約に起きた出来事を読み取る
    ///
    /// # Arguments
    /// * `repository` - 見えなくなった予約を確かめるための予約リポジトリ
    /// * `previous` - 前回見えていた予約
    /// * `current` - 今回見えている予約
    /// * `now` - 今回の予約を読んだ時刻。これより前に終わる予定だった予約は、
    ///   予定どおり終わったものとして扱う
    ///
    /// # Errors
    /// 見えなくなった予約の確認でリポジトリエラーが起きた場合
    pub async fn interpret<R: ReservationRepository + ?Sized>(
        &self,
        repository: &R,
        previous: &HashMap<ReservationId, Reservation>,
        current: &HashMap<ReservationId, Reservation>,
        now: DateTime<Utc>,
    ) -> Result<InterpretedChanges, RepositoryError> {
        let mut changes = InterpretedChanges::default();

        for (id, reservation) in current {
            match previous.get(id) {
                None => {
                    changes.appeared += 1;
                    changes
                        .events
                        .push(ReservationEvent::Reserved(reservation.clone()));
                }
                Some(before) if !before.has_same_content_as(reservation) => {
                    changes
                        .events
                        .extend(Self::changes(before, reservation, now));
                }
                Some(_) => {}
            }
        }

        for (id, before) in previous {
            if current.contains_key(id) || Self::ran_its_course(before, now) {
                continue;
            }
            changes.disappeared += 1;
            match repository.find(id).await? {
                Some(after) => {
                    // 内容を変えずに監視している期間から外れただけなら、何も起きていない
                    if !after.has_same_content_as(before) {
                        changes.events.extend(Self::changes(before, &after, now));
                    }
                    changes.carried_over.push(after);
                }
                None => changes
                    .events
                    .push(ReservationEvent::Cancelled(before.clone())),
            }
        }

        Ok(changes)
    }

    /// 内容が変わった予約の出来事
    ///
    /// 定期予約で、予約そのもの（予約者・リソース・備考・最初の予約枠・繰り返しの規則）は
    /// 変わらず一部の回だけが変わったなら、回ごとの出来事にする。それ以外は予約全体の変化。
    fn changes(
        before: &Reservation,
        after: &Reservation,
        now: DateTime<Utc>,
    ) -> Vec<ReservationEvent> {
        match (before.schedule(), after.schedule()) {
            (Schedule::Recurring(was), Schedule::Recurring(is))
                if Self::same_series(before, after, was, is) =>
            {
                Self::occurrence_changes(before, after, was, is, now)
            }
            _ => vec![Self::change(before, after, now)],
        }
    }

    /// 定期予約そのものは変わっていないか（回ごとの例外は見ない）
    fn same_series(
        before: &Reservation,
        after: &Reservation,
        was: &RecurringSchedule,
        is: &RecurringSchedule,
    ) -> bool {
        before.holder_email() == after.holder_email()
            && before.resources() == after.resources()
            && before.notes() == after.notes()
            && before.status() == after.status()
            && was.first() == is.first()
            && was.rule() == is.rule()
            && was.time_zone() == is.time_zone()
    }

    /// 変わった回ごとの出来事（回の本来の開始順）
    fn occurrence_changes(
        before: &Reservation,
        after: &Reservation,
        was: &RecurringSchedule,
        is: &RecurringSchedule,
        now: DateTime<Utc>,
    ) -> Vec<ReservationEvent> {
        let starts: BTreeSet<DateTime<Utc>> = was
            .exceptions()
            .keys()
            .chain(is.exceptions().keys())
            .copied()
            .collect();
        starts
            .into_iter()
            .filter(|start| {
                was.exceptions().get(start).map(Exception::change)
                    != is.exceptions().get(start).map(Exception::change)
            })
            .filter_map(|start| {
                let key = OccurrenceKey::At(start);
                match (before.slot(&key), after.slot(&key)) {
                    (Some(slot), None) => Some(ReservationEvent::SlotCancelled {
                        reservation: after.clone(),
                        slot,
                    }),
                    (was_slot, Some(slot)) => {
                        let reservation = after.clone();
                        let ended_early = was_slot.is_some_and(|was_slot| {
                            Self::shortened_into_the_past(
                                was_slot.time_period(),
                                slot.time_period(),
                                now,
                            )
                        });
                        Some(if ended_early {
                            ReservationEvent::SlotEndedEarly { reservation, slot }
                        } else {
                            ReservationEvent::SlotChanged { reservation, slot }
                        })
                    }
                    (None, None) => None,
                }
            })
            .collect()
    }

    /// 予約全体の変化の出来事
    fn change(before: &Reservation, after: &Reservation, now: DateTime<Utc>) -> ReservationEvent {
        if Self::ended_early(before, after, now) {
            ReservationEvent::EndedEarly(after.clone())
        } else {
            ReservationEvent::Changed(after.clone())
        }
    }

    /// 単発予約の予約枠が、始まりはそのままに、終わりだけ今より前へ縮んだか
    fn ended_early(before: &Reservation, after: &Reservation, now: DateTime<Utc>) -> bool {
        match (before.schedule(), after.schedule()) {
            (Schedule::OneTime(was), Schedule::OneTime(is)) => {
                Self::shortened_into_the_past(was, is, now)
            }
            _ => false,
        }
    }

    /// 始まりはそのままに、終わりだけ今より前へ縮んだか
    fn shortened_into_the_past(was: &TimePeriod, is: &TimePeriod, now: DateTime<Utc>) -> bool {
        was.start() == is.start() && is.end() < was.end() && is.end() <= now
    }

    /// 予約の最後の予約枠が、予定どおり終わったか
    fn ran_its_course(reservation: &Reservation, now: DateTime<Utc>) -> bool {
        reservation.last_end().is_some_and(|end| end <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::common::{EmailAddress, TimePeriod};
    use async_trait::async_trait;
    use chrono::Duration;

    /// 見えなくなった予約を確かめるときに、保存先に残っている予約だけを返すスタブ
    struct Stored(Vec<Reservation>);

    #[async_trait]
    impl ReservationRepository for Stored {
        async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError> {
            Ok(self.0.iter().find(|r| r.id() == id).cloned())
        }

        async fn find_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<Reservation>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save(&self, _reservation: &mut Reservation) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn reservation(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Reservation {
        Reservation::reconstruct(
            ReservationId::from_string(id.to_string()),
            holder(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
            Schedule::OneTime(TimePeriod::new(start, end).unwrap()),
            None,
        )
        .unwrap()
    }

    fn seen(reservations: &[&Reservation]) -> HashMap<ReservationId, Reservation> {
        reservations
            .iter()
            .map(|r| (r.id().clone(), (*r).clone()))
            .collect()
    }

    async fn interpret(
        stored: Vec<Reservation>,
        previous: &[&Reservation],
        current: &[&Reservation],
        now: DateTime<Utc>,
    ) -> InterpretedChanges {
        ReservationChangeInterpreter::new()
            .interpret(&Stored(stored), &seen(previous), &seen(current), now)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_reservation_that_appears_was_reserved() {
        let now = Utc::now();
        let new = reservation("new", now + Duration::hours(1), now + Duration::hours(2));

        let changes = interpret(vec![], &[], &[&new], now).await;

        assert_eq!(changes.appeared, 1);
        assert_eq!(changes.events, vec![ReservationEvent::Reserved(new)]);
    }

    #[tokio::test]
    async fn a_reservation_whose_contents_differ_has_changed() {
        let now = Utc::now();
        let before = reservation("r", now + Duration::hours(1), now + Duration::hours(2));
        let after = reservation("r", now + Duration::hours(1), now + Duration::hours(3));

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Changed(after)]);
    }

    #[tokio::test]
    async fn only_a_new_version_is_not_an_event() {
        let now = Utc::now();
        let before = reservation("r", now + Duration::hours(1), now + Duration::hours(2));
        let mut after = before.clone();
        after.record_version(Some(crate::domain::aggregates::reservation::Version::new(
            "\"2\"".to_string(),
        )));

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert!(changes.events.is_empty());
    }

    #[tokio::test]
    async fn a_reservation_that_left_the_window_with_an_earlier_end_ended_early() {
        let now = Utc::now();
        let running = reservation("r", now - Duration::hours(1), now + Duration::hours(2));
        let ended = reservation("r", now - Duration::hours(1), now - Duration::minutes(1));

        let changes = interpret(vec![ended.clone()], &[&running], &[], now).await;

        assert_eq!(changes.disappeared, 1);
        assert_eq!(
            changes.events,
            vec![ReservationEvent::EndedEarly(ended)],
            "使った分が予約枠として残っているなら、取り消しではない"
        );
    }

    #[tokio::test]
    async fn a_reservation_that_is_no_longer_stored_was_cancelled() {
        let now = Utc::now();
        let upcoming = reservation("r", now + Duration::hours(1), now + Duration::hours(2));

        let changes = interpret(vec![], &[&upcoming], &[], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Cancelled(upcoming)]);
    }

    #[tokio::test]
    async fn a_reservation_moved_beyond_the_window_has_changed() {
        let now = Utc::now();
        let soon = reservation("r", now + Duration::hours(1), now + Duration::hours(2));
        let much_later = reservation(
            "r",
            now + Duration::days(90),
            now + Duration::days(90) + Duration::hours(1),
        );

        let changes = interpret(vec![much_later.clone()], &[&soon], &[], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Changed(much_later)]);
    }

    #[tokio::test]
    async fn a_reservation_that_ran_its_course_is_not_an_event() {
        let now = Utc::now();
        let finished = reservation("r", now - Duration::hours(2), now - Duration::minutes(1));

        let changes = interpret(vec![finished.clone()], &[&finished], &[], now).await;

        assert_eq!(changes, InterpretedChanges::default());
    }
}

#[cfg(test)]
mod recurring_tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::aggregates::reservation::{RecurrenceRule, RecurringSchedule};
    use crate::domain::common::EmailAddress;
    use async_trait::async_trait;
    use chrono::Duration;
    use std::collections::BTreeMap;

    struct Stored(Vec<Reservation>);

    #[async_trait]
    impl ReservationRepository for Stored {
        async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError> {
            Ok(self.0.iter().find(|r| r.id() == id).cloned())
        }

        async fn find_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<Reservation>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save(&self, _reservation: &mut Reservation) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    /// 毎日、`first_start`から2時間の定期予約
    fn daily(rule: &str, first_start: DateTime<Utc>) -> Reservation {
        Reservation::reconstruct(
            ReservationId::from_string("series".to_string()),
            holder(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
            Schedule::Recurring(RecurringSchedule::new(
                TimePeriod::new(first_start, first_start + Duration::hours(2)).unwrap(),
                RecurrenceRule::parse(vec![rule.to_string()]).unwrap(),
                chrono_tz::UTC,
                BTreeMap::new(),
            )),
            None,
        )
        .unwrap()
    }

    /// 毎日の定期予約。最初の回は、今の時刻の1時間前に始まる（今は2回目以降も続く）
    fn running_daily(now: DateTime<Utc>) -> Reservation {
        daily("RRULE:FREQ=DAILY", now - Duration::hours(1))
    }

    fn seen(reservations: &[&Reservation]) -> HashMap<ReservationId, Reservation> {
        reservations
            .iter()
            .map(|r| (r.id().clone(), (*r).clone()))
            .collect()
    }

    async fn interpret(
        stored: Vec<Reservation>,
        previous: &[&Reservation],
        current: &[&Reservation],
        now: DateTime<Utc>,
    ) -> InterpretedChanges {
        ReservationChangeInterpreter::new()
            .interpret(&Stored(stored), &seen(previous), &seen(current), now)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_new_recurring_reservation_is_one_event_not_one_per_slot() {
        let now = Utc::now();
        let series = running_daily(now);

        let changes = interpret(vec![], &[], &[&series], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Reserved(series)]);
    }

    #[tokio::test]
    async fn changing_one_occurrence_is_an_event_about_that_slot() {
        let now = Utc::now();
        let before = running_daily(now);
        let tomorrow = OccurrenceKey::At(now - Duration::hours(1) + Duration::days(1));
        let mut after = before.clone();
        after
            .change_slot_notes(&tomorrow, "この回だけ".to_string(), &holder())
            .unwrap();

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert_eq!(
            changes.events,
            vec![ReservationEvent::SlotChanged {
                reservation: after.clone(),
                slot: after.slot(&tomorrow).unwrap(),
            }]
        );
    }

    #[tokio::test]
    async fn cancelling_one_occurrence_is_reported_with_the_slot_that_went() {
        let now = Utc::now();
        let before = running_daily(now);
        let tomorrow = OccurrenceKey::At(now - Duration::hours(1) + Duration::days(1));
        let mut after = before.clone();
        after.cancel_slot(&tomorrow, &holder()).unwrap();

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert_eq!(
            changes.events,
            vec![ReservationEvent::SlotCancelled {
                reservation: after,
                slot: before.slot(&tomorrow).unwrap(),
            }]
        );
    }

    #[tokio::test]
    async fn ending_the_running_occurrence_early_is_not_a_cancellation() {
        let now = Utc::now();
        let before = running_daily(now);
        let today = OccurrenceKey::At(now - Duration::hours(1));
        let mut after = before.clone();
        after
            .end_slot_early(&today, now - Duration::minutes(1), &holder())
            .unwrap();

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert!(
            matches!(
                changes.events.as_slice(),
                [ReservationEvent::SlotEndedEarly { slot, .. }] if slot.key() == &today
            ),
            "{:?}",
            changes.events
        );
    }

    #[tokio::test]
    async fn changing_the_rule_is_a_change_of_the_whole_reservation() {
        let now = Utc::now();
        let before = running_daily(now);
        let after = daily("RRULE:FREQ=DAILY;INTERVAL=2", now - Duration::hours(1));

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Changed(after)]);
    }

    #[tokio::test]
    async fn a_recurring_reservation_that_only_left_the_window_is_not_an_event() {
        let now = Utc::now();
        let series = running_daily(now);

        let changes = interpret(vec![series.clone()], &[&series], &[], now).await;

        assert!(changes.events.is_empty(), "{:?}", changes.events);
        assert_eq!(
            changes.carried_over,
            vec![series],
            "まだ続いている予約として持ち越す"
        );
    }

    #[tokio::test]
    async fn a_carried_over_reservation_coming_back_into_the_window_is_not_new() {
        let now = Utc::now();
        let series = running_daily(now);
        // 前回の読み取りで、監視している期間から外れて持ち越された
        let left = interpret(vec![series.clone()], &[&series], &[], now).await;
        let carried: Vec<&Reservation> = left.carried_over.iter().collect();

        let back = interpret(vec![series.clone()], &carried, &[&series], now).await;

        assert!(
            back.events.is_empty(),
            "予約枠が監視期間に戻ってきても、新しい予約ではない: {:?}",
            back.events
        );
        assert_eq!(back.appeared, 0);
    }

    #[tokio::test]
    async fn a_deleted_recurring_reservation_is_cancelled_once() {
        let now = Utc::now();
        let series = running_daily(now);

        let changes = interpret(vec![], &[&series], &[], now).await;

        assert_eq!(changes.events, vec![ReservationEvent::Cancelled(series)]);
    }

    #[tokio::test]
    async fn a_recurring_reservation_whose_last_slot_ended_ran_its_course() {
        let now = Utc::now();
        let finished = daily("RRULE:FREQ=DAILY;COUNT=2", now - Duration::days(3));

        let changes = interpret(vec![], &[&finished], &[], now).await;

        assert_eq!(changes, InterpretedChanges::default());
    }
}
