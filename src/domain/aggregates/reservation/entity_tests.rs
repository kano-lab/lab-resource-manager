//! `Reservation`のテスト

use super::*;
use crate::domain::aggregates::reservation::value_objects::Gpu;
use chrono::Duration;

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
    let start = Utc::now() - Duration::hours(1);
    (one_time(start, start + Duration::hours(4)), start)
}

#[test]
fn a_new_one_time_reservation_is_active_unsaved_and_has_one_slot() {
    let start = Utc::now() + Duration::hours(1);
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
    let start = Utc::now();
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
    let start = Utc::now() + Duration::days(3);
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
    let start = Utc::now() + Duration::hours(1);
    let mut reservation = one_time(start, start + Duration::hours(2));

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, Utc::now(), &holder())
        .unwrap_err();

    assert!(
        matches!(error, ReservationError::NotYetStarted { .. }),
        "使った時間がない予約枠は締められない（取り消しの領分）: {error:?}"
    );
}

#[test]
fn a_slot_that_has_already_ended_cannot_end_early() {
    let end = Utc::now() - Duration::hours(1);
    let mut reservation = one_time(end - Duration::hours(2), end);

    let error = reservation
        .end_slot_early(&OccurrenceKey::Only, Utc::now(), &holder())
        .unwrap_err();

    assert!(matches!(error, ReservationError::AlreadyEnded { .. }));
}

#[test]
fn the_holder_can_reschedule_and_rewrite_the_notes() {
    let start = Utc::now() + Duration::hours(1);
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
        reservation
            .reschedule_slot(&OccurrenceKey::Only, later, &someone_else())
            .unwrap_err(),
        reservation
            .change_slot_notes(&OccurrenceKey::Only, "横取り".to_string(), &someone_else())
            .unwrap_err(),
        reservation
            .end_slot_early(&OccurrenceKey::Only, Utc::now(), &someone_else())
            .unwrap_err(),
        reservation
            .cancel_slot(&OccurrenceKey::Only, &someone_else())
            .unwrap_err(),
        reservation.cancel(&someone_else()).unwrap_err(),
    ];

    for error in refused {
        assert!(
            matches!(&error, ReservationError::NotHolder { actor } if actor == &someone_else()),
            "{error:?}"
        );
    }
    assert_eq!(reservation, untouched);
}

#[test]
fn cancelling_the_slot_of_a_one_time_reservation_cancels_the_reservation() {
    let start = Utc::now() + Duration::hours(1);
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
        reservation
            .reschedule_slot(&OccurrenceKey::Only, later, &holder())
            .unwrap_err(),
        reservation
            .end_slot_early(&OccurrenceKey::Only, Utc::now(), &holder())
            .unwrap_err(),
        reservation.cancel(&holder()).unwrap_err(),
    ];

    for error in refused {
        assert!(
            matches!(error, ReservationError::AlreadyCancelled),
            "{error:?}"
        );
    }
}

#[test]
fn the_version_is_not_part_of_the_content() {
    let start = Utc::now() + Duration::hours(1);
    let reservation = one_time(start, start + Duration::hours(2));
    let mut saved = reservation.clone();
    saved.record_version(Some(Version::new("\"2\"".to_string())));

    assert!(saved.has_same_content_as(&reservation));
    assert_ne!(saved, reservation);
}
