//! `Reservation`のテスト

use super::*;
use crate::domain::aggregates::reservation::value_objects::Gpu;
use chrono::{Duration, TimeZone};

/// テストの「今」。2026年11月16日（月）10時（東京）
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap()
}

fn holder() -> EmailAddress {
    EmailAddress::new("holder@example.com".to_string()).unwrap()
}

fn someone_else() -> EmailAddress {
    EmailAddress::new("other@example.com".to_string()).unwrap()
}

fn gpu() -> Resource {
    Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()))
}

fn one_time(start: DateTime<Utc>, end: DateTime<Utc>) -> Reservation {
    Reservation::one_time(
        holder(),
        TimePeriod::new(start, end).unwrap(),
        vec![gpu()],
        Some("学習".to_string()),
    )
    .unwrap()
}

fn running() -> (Reservation, DateTime<Utc>) {
    let start = now() - Duration::hours(1);
    (one_time(start, start + Duration::hours(4)), start)
}

#[test]
fn a_new_one_time_reservation_is_active_unsaved_and_has_one_slot() {
    let start = now() + Duration::hours(1);
    let reservation = one_time(start, start + Duration::hours(2));

    assert_eq!(reservation.status(), ReservationStatus::Active);
    assert!(reservation.version().is_none(), "まだ保存していない");

    let slot = reservation.slot(&OccurrenceKey::Only).unwrap();
    assert_eq!(slot.time_period().start(), start);
    assert_eq!(slot.notes().map(String::as_str), Some("学習"));
    assert_eq!(
        reservation.slot_id(OccurrenceKey::Only).as_str(),
        reservation.id().as_str(),
        "単発予約の予約枠IDは予約IDと同じ文字列"
    );
}

#[test]
fn a_reservation_must_hold_at_least_one_resource() {
    let start = now();
    let error = Reservation::one_time(
        holder(),
        TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
        vec![],
        None,
    )
    .unwrap_err();

    assert!(matches!(error, ReservationError::NoResourceItems));
}

#[test]
fn slots_in_returns_only_slots_overlapping_the_window() {
    let start = now() + Duration::days(3);
    let reservation = one_time(start, start + Duration::hours(2));
    let this_week = TimePeriod::new(start - Duration::days(1), start + Duration::days(1)).unwrap();
    let next_month =
        TimePeriod::new(start + Duration::days(30), start + Duration::days(31)).unwrap();

    assert_eq!(reservation.slots_in(&this_week).len(), 1);
    assert!(reservation.slots_in(&next_month).is_empty());
}

#[test]
fn ending_early_shortens_the_slot_to_that_moment() {
    let (mut reservation, start) = running();
    let now = start + Duration::hours(1);

    let event = reservation
        .end_slot_early(&OccurrenceKey::Only, now, &holder())
        .unwrap();

    let slot = reservation.slot(&OccurrenceKey::Only).unwrap();
    assert_eq!(slot.time_period().start(), start, "使い始めた時刻は残る");
    assert_eq!(slot.time_period().end(), now);
    assert_eq!(event, ReservationEvent::EndedEarly(reservation.clone()));
}

#[test]
fn a_slot_that_has_not_started_cannot_end_early() {
    let start = now() + Duration::hours(1);
    let mut reservation = one_time(start, start + Duration::hours(2));

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, now(), &holder())
        .unwrap_err();

    assert!(
        matches!(error, ReservationError::NotYetStarted { .. }),
        "使った時間がない予約枠は締められない（取り消しの領分）: {error:?}"
    );
}

#[test]
fn a_slot_that_has_already_ended_cannot_end_early() {
    let end = now() - Duration::hours(1);
    let mut reservation = one_time(end - Duration::hours(2), end);

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, now(), &holder())
        .unwrap_err();

    assert!(matches!(error, ReservationError::AlreadyEnded { .. }));
}

#[test]
fn ending_early_at_the_very_start_is_refused() {
    let start = now();
    let mut reservation = one_time(start, start + Duration::hours(2));

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, start, &holder())
        .unwrap_err();

    assert!(
        matches!(error, ReservationError::NotYetStarted { .. }),
        "始まった瞬間には、使った時間がまだない: {error:?}"
    );
}

#[test]
fn ending_early_a_moment_after_the_start_keeps_that_moment() {
    let start = now();
    let mut reservation = one_time(start, start + Duration::hours(2));
    let a_moment_later = start + Duration::nanoseconds(1);

    reservation
        .end_slot_early(&OccurrenceKey::Only, a_moment_later, &holder())
        .unwrap();

    let slot = reservation.slot(&OccurrenceKey::Only).unwrap();
    assert_eq!(slot.time_period().end(), a_moment_later);
}

#[test]
fn ending_early_at_the_very_end_is_refused() {
    let end = now();
    let mut reservation = one_time(end - Duration::hours(2), end);

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, end, &holder())
        .unwrap_err();

    assert!(
        matches!(error, ReservationError::AlreadyEnded { .. }),
        "終わる時刻ちょうどには、もう終わっている: {error:?}"
    );
}

#[test]
fn the_holder_can_reschedule_and_rewrite_the_notes() {
    let start = now() + Duration::hours(1);
    let mut reservation = one_time(start, start + Duration::hours(2));
    let later = TimePeriod::new(start + Duration::hours(3), start + Duration::hours(5)).unwrap();

    reservation
        .reschedule_slot(&OccurrenceKey::Only, later.clone(), &holder())
        .unwrap();
    let event = reservation
        .change_slot_notes(&OccurrenceKey::Only, "続き".to_string(), &holder())
        .unwrap();

    let slot = reservation.slot(&OccurrenceKey::Only).unwrap();
    assert_eq!(slot.time_period(), &later);
    assert_eq!(slot.notes().map(String::as_str), Some("続き"));
    assert_eq!(event, ReservationEvent::Changed(reservation.clone()));
}

#[test]
fn only_the_holder_can_operate_and_a_refusal_changes_nothing() {
    let (mut reservation, start) = running();
    let untouched = reservation.clone();
    let later = TimePeriod::new(start, start + Duration::hours(6)).unwrap();

    let refused = [
        (
            "reschedule_slot",
            reservation
                .reschedule_slot(&OccurrenceKey::Only, later, &someone_else())
                .unwrap_err(),
        ),
        (
            "change_slot_notes",
            reservation
                .change_slot_notes(&OccurrenceKey::Only, "横取り".to_string(), &someone_else())
                .unwrap_err(),
        ),
        (
            "end_slot_early",
            reservation
                .end_slot_early(&OccurrenceKey::Only, now(), &someone_else())
                .unwrap_err(),
        ),
        (
            "cancel_slot",
            reservation
                .cancel_slot(&OccurrenceKey::Only, &someone_else())
                .unwrap_err(),
        ),
        ("cancel", reservation.cancel(&someone_else()).unwrap_err()),
    ];

    for (operation, error) in refused {
        assert!(
            matches!(&error, ReservationError::NotHolder { actor } if actor == &someone_else()),
            "{operation}: {error:?}"
        );
    }
    assert_eq!(reservation, untouched);
}

#[test]
fn cancelling_the_slot_of_a_one_time_reservation_cancels_the_reservation() {
    let start = now() + Duration::hours(1);
    let mut reservation = one_time(start, start + Duration::hours(2));

    let event = reservation
        .cancel_slot(&OccurrenceKey::Only, &holder())
        .unwrap();

    assert_eq!(reservation.status(), ReservationStatus::Cancelled);
    assert_eq!(event, ReservationEvent::Cancelled(reservation.clone()));
    assert!(
        reservation.slot(&OccurrenceKey::Only).is_none(),
        "取り消した予約には予約枠が無い"
    );
}

#[test]
fn a_cancelled_reservation_accepts_no_more_operations() {
    let (mut reservation, start) = running();
    reservation.cancel(&holder()).unwrap();
    let later = TimePeriod::new(start, start + Duration::hours(6)).unwrap();

    let refused = [
        (
            "reschedule_slot",
            reservation
                .reschedule_slot(&OccurrenceKey::Only, later, &holder())
                .unwrap_err(),
        ),
        (
            "change_slot_notes",
            reservation
                .change_slot_notes(&OccurrenceKey::Only, "続き".to_string(), &holder())
                .unwrap_err(),
        ),
        (
            "end_slot_early",
            reservation
                .end_slot_early(&OccurrenceKey::Only, now(), &holder())
                .unwrap_err(),
        ),
        (
            "cancel_slot",
            reservation
                .cancel_slot(&OccurrenceKey::Only, &holder())
                .unwrap_err(),
        ),
        ("cancel", reservation.cancel(&holder()).unwrap_err()),
    ];

    for (operation, error) in refused {
        assert!(
            matches!(error, ReservationError::AlreadyCancelled),
            "{operation}: {error:?}"
        );
    }
}

#[test]
fn the_version_is_not_part_of_the_content() {
    let start = now() + Duration::hours(1);
    let reservation = one_time(start, start + Duration::hours(2));
    let mut saved = reservation.clone();
    saved.record_version(Some(Version::new("\"2\"".to_string())));

    assert!(saved.has_same_content_as(&reservation));
    assert_ne!(saved, reservation);
}

mod recurring {
    use super::*;
    use crate::domain::aggregates::reservation::recurring::{
        Exception, ExceptionChange, RecurringSchedule,
    };
    use crate::domain::aggregates::reservation::value_objects::RecurrenceRule;
    use chrono::TimeZone;
    use chrono_tz::Tz;
    use std::collections::BTreeMap;

    fn tokyo() -> Tz {
        "Asia/Tokyo".parse().unwrap()
    }

    /// 東京の時刻をUTCで
    fn jst(m: u32, d: u32, h: u32) -> DateTime<Utc> {
        tokyo()
            .with_ymd_and_hms(2026, m, d, h, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// 毎週月曜10時から2時間の定期予約（最初の回は2026年11月16日）
    fn weekly(rule: &str, exceptions: BTreeMap<DateTime<Utc>, Exception>) -> Reservation {
        let first = TimePeriod::new(jst(11, 16, 10), jst(11, 16, 12)).unwrap();
        Reservation::reconstruct(
            ReservationId::from_string("series".to_string()),
            holder(),
            vec![gpu()],
            Some("週次ミーティング".to_string()),
            Schedule::Recurring(RecurringSchedule::new(
                first,
                RecurrenceRule::parse(vec![rule.to_string()]).unwrap(),
                tokyo(),
                exceptions,
            )),
            Some(Version::new("\"1\"".to_string())),
        )
        .unwrap()
    }

    fn every_monday() -> Reservation {
        weekly("RRULE:FREQ=WEEKLY;BYDAY=MO", BTreeMap::new())
    }

    fn at(m: u32, d: u32) -> OccurrenceKey {
        OccurrenceKey::At(jst(m, d, 10))
    }

    fn period(from: DateTime<Utc>, to: DateTime<Utc>) -> TimePeriod {
        TimePeriod::new(from, to).unwrap()
    }

    #[test]
    fn each_occurrence_is_a_slot_identified_by_its_original_start() {
        let reservation = every_monday();

        let slots = reservation.slots_in(&period(jst(11, 20, 0), jst(12, 5, 0)));

        assert_eq!(
            slots
                .iter()
                .map(|slot| slot.key().clone())
                .collect::<Vec<_>>(),
            vec![at(11, 23), at(11, 30)]
        );
        assert_eq!(
            slots[0].time_period(),
            &period(jst(11, 23, 10), jst(11, 23, 12)),
            "長さは最初の予約枠と同じ"
        );
        assert_eq!(
            slots[0].notes().map(String::as_str),
            Some("週次ミーティング")
        );
        assert_eq!(
            reservation.slot_id(at(11, 23)).as_str(),
            "series_20261123T010000Z"
        );
        assert!(
            reservation
                .slot(&OccurrenceKey::At(jst(11, 24, 10)))
                .is_none(),
            "規則に無い回は無い"
        );
        assert!(reservation.slot(&OccurrenceKey::Only).is_none());
    }

    #[test]
    fn rescheduling_one_occurrence_leaves_the_others_alone() {
        let mut reservation = every_monday();
        let later = period(jst(11, 23, 14), jst(11, 23, 16));

        let event = reservation
            .reschedule_slot(&at(11, 23), later.clone(), &holder())
            .unwrap();

        let slot = reservation.slot(&at(11, 23)).unwrap();
        assert_eq!(slot.time_period(), &later);
        assert_eq!(
            reservation.slot(&at(11, 30)).unwrap().time_period(),
            &period(jst(11, 30, 10), jst(11, 30, 12))
        );
        assert_eq!(
            event,
            ReservationEvent::SlotChanged {
                reservation: reservation.clone(),
                slot
            }
        );
        assert_eq!(reservation.status(), ReservationStatus::Active);
    }

    #[test]
    fn a_moved_occurrence_is_found_where_it_was_moved_to() {
        let mut reservation = every_monday();
        // 11月23日の回を12月10日（水）へ動かす。本来の時刻は窓の外
        reservation
            .reschedule_slot(
                &at(11, 23),
                period(jst(12, 10, 10), jst(12, 10, 12)),
                &holder(),
            )
            .unwrap();

        let slots = reservation.slots_in(&period(jst(12, 9, 0), jst(12, 11, 0)));

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].key(), &at(11, 23), "鍵は本来の開始時刻のまま");
    }

    #[test]
    fn cancelling_one_occurrence_keeps_the_reservation_and_its_other_slots() {
        let mut reservation = every_monday();
        let before = reservation.slot(&at(11, 23)).unwrap();

        let event = reservation.cancel_slot(&at(11, 23), &holder()).unwrap();

        assert!(reservation.slot(&at(11, 23)).is_none());
        assert!(reservation.slot(&at(11, 30)).is_some());
        assert_eq!(reservation.status(), ReservationStatus::Active);
        assert_eq!(
            event,
            ReservationEvent::SlotCancelled {
                reservation: reservation.clone(),
                slot: before
            },
            "取り消した予約枠は取り消す前の姿で伝える"
        );
        let error = reservation.cancel_slot(&at(11, 23), &holder()).unwrap_err();
        assert!(
            matches!(error, ReservationError::NoSuchSlot(_)),
            "{error:?}"
        );
    }

    #[test]
    fn ending_a_running_occurrence_early_shortens_only_that_slot() {
        let mut reservation = every_monday();
        let now = jst(11, 23, 11);

        let event = reservation
            .end_slot_early(&at(11, 23), now, &holder())
            .unwrap();

        let slot = reservation.slot(&at(11, 23)).unwrap();
        assert_eq!(slot.time_period(), &period(jst(11, 23, 10), now));
        assert!(matches!(event, ReservationEvent::SlotEndedEarly { .. }));
    }

    #[test]
    fn cancelling_the_reservation_cancels_every_slot() {
        let mut reservation = every_monday();

        let event = reservation.cancel(&holder()).unwrap();

        assert_eq!(event, ReservationEvent::Cancelled(reservation.clone()));
        assert!(
            reservation
                .slots_in(&period(jst(11, 1, 0), jst(12, 31, 0)))
                .is_empty()
        );
    }

    #[test]
    fn only_the_holder_can_operate_on_an_occurrence() {
        let mut reservation = every_monday();

        let error = reservation
            .cancel_slot(&at(11, 23), &someone_else())
            .unwrap_err();

        assert!(matches!(error, ReservationError::NotHolder { .. }));
        assert!(reservation.slot(&at(11, 23)).is_some());
    }

    #[test]
    fn changing_an_occurrence_keeps_the_version_it_was_read_at() {
        let read_at = Version::new("\"7\"".to_string());
        let mut exceptions = BTreeMap::new();
        exceptions.insert(
            jst(11, 23, 10),
            Exception::new(
                ExceptionChange::Changed {
                    time_period: period(jst(11, 23, 13), jst(11, 23, 15)),
                    notes: None,
                },
                Some(read_at.clone()),
            ),
        );
        let mut reservation = weekly("RRULE:FREQ=WEEKLY;BYDAY=MO", exceptions);

        reservation.cancel_slot(&at(11, 23), &holder()).unwrap();

        let Schedule::Recurring(recurring) = reservation.schedule() else {
            panic!("定期予約のはず");
        };
        let exception = &recurring.exceptions()[&jst(11, 23, 10)];
        assert_eq!(exception.change(), &ExceptionChange::Cancelled);
        assert_eq!(
            exception.version(),
            Some(&read_at),
            "保存先で読んだあとに変わっていないかを確かめるのに使う"
        );
        assert_eq!(
            recurring.unsaved_occurrences().iter().collect::<Vec<_>>(),
            vec![&jst(11, 23, 10)],
            "保存するのは変えた回だけ"
        );

        reservation.record_occurrence_version(jst(11, 23, 10), None);
        let Schedule::Recurring(recurring) = reservation.schedule() else {
            panic!("定期予約のはず");
        };
        assert!(recurring.unsaved_occurrences().is_empty());
    }

    #[test]
    fn versions_of_occurrences_are_not_part_of_the_content() {
        let reservation = every_monday();
        let mut other = reservation.clone();
        other.record_version(Some(Version::new("\"2\"".to_string())));
        let mut changed = reservation.clone();
        changed
            .change_slot_notes(&at(11, 23), "特別回".to_string(), &holder())
            .unwrap();
        let mut saved = changed.clone();
        saved.record_occurrence_version(jst(11, 23, 10), Some(Version::new("\"3\"".to_string())));

        assert!(other.has_same_content_as(&reservation));
        assert!(!changed.has_same_content_as(&reservation));
        assert!(saved.has_same_content_as(&changed));
    }

    #[test]
    fn a_reservation_without_an_end_has_no_last_slot() {
        let endless = every_monday();
        let three_times = weekly("RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=3", BTreeMap::new());

        assert_eq!(endless.last_end(), None);
        assert_eq!(three_times.last_end(), Some(jst(11, 30, 12)));
    }

    /// 予約枠を指す鍵ひとつに、4つの操作を試す
    fn every_operation_on(
        reservation: &mut Reservation,
        key: &OccurrenceKey,
    ) -> Vec<(&'static str, ReservationError)> {
        let later = period(jst(12, 24, 14), jst(12, 24, 16));
        vec![
            (
                "reschedule_slot",
                reservation
                    .reschedule_slot(key, later, &holder())
                    .unwrap_err(),
            ),
            (
                "change_slot_notes",
                reservation
                    .change_slot_notes(key, "特別回".to_string(), &holder())
                    .unwrap_err(),
            ),
            (
                "end_slot_early",
                reservation
                    .end_slot_early(key, jst(11, 24, 11), &holder())
                    .unwrap_err(),
            ),
            (
                "cancel_slot",
                reservation.cancel_slot(key, &holder()).unwrap_err(),
            ),
        ]
    }

    #[test]
    fn an_occurrence_the_rule_does_not_have_cannot_be_operated_on() {
        let mut reservation = every_monday();
        let untouched = reservation.clone();

        // 火曜の10時と、単発予約の鍵。どちらもこの定期予約の予約枠ではない
        for key in [OccurrenceKey::At(jst(11, 24, 10)), OccurrenceKey::Only] {
            for (operation, error) in every_operation_on(&mut reservation, &key) {
                assert!(
                    matches!(error, ReservationError::NoSuchSlot(_)),
                    "{key:?} {operation}: {error:?}"
                );
            }
        }
        assert_eq!(reservation, untouched, "断った操作は何も変えない");
    }

    #[test]
    fn a_cancelled_occurrence_cannot_be_operated_on() {
        let mut reservation = every_monday();
        reservation.cancel_slot(&at(11, 23), &holder()).unwrap();
        let after_cancelling = reservation.clone();

        for (operation, error) in every_operation_on(&mut reservation, &at(11, 23)) {
            assert!(
                matches!(error, ReservationError::NoSuchSlot(_)),
                "{operation}: {error:?}"
            );
        }
        assert_eq!(reservation, after_cancelling);
    }

    #[test]
    fn a_moved_occurrence_can_be_moved_again() {
        let mut reservation = every_monday();
        reservation
            .reschedule_slot(
                &at(11, 23),
                period(jst(11, 24, 10), jst(11, 24, 12)),
                &holder(),
            )
            .unwrap();

        reservation
            .reschedule_slot(
                &at(11, 23),
                period(jst(11, 25, 10), jst(11, 25, 12)),
                &holder(),
            )
            .unwrap();

        assert_eq!(
            reservation.slot(&at(11, 23)).unwrap().time_period(),
            &period(jst(11, 25, 10), jst(11, 25, 12)),
            "鍵は本来の開始時刻のまま、2度目の移動先にある"
        );
        assert!(
            reservation
                .slots_in(&period(jst(11, 24, 0), jst(11, 25, 0)))
                .is_empty(),
            "1度目の移動先には何も残らない"
        );
    }

    #[test]
    fn a_slot_that_only_touches_the_window_is_not_in_it() {
        let reservation = every_monday();

        // 11月23日の回は10時から12時
        let right_after = period(jst(11, 23, 12), jst(11, 23, 13));
        let right_before = period(jst(11, 23, 9), jst(11, 23, 10));

        assert!(reservation.slots_in(&right_after).is_empty());
        assert!(reservation.slots_in(&right_before).is_empty());
    }
}
