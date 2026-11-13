//! 予約リポジトリの契約テスト
//!
//! `ReservationRepository`と`ReservedSlotQuery`の約束を、どの実装にも同じシナリオで確かめる。
//! ユースケースのテストはインメモリの実装で書かれているので、Google カレンダーの実装も
//! 同じように振る舞うことをここで保証する（Google カレンダーの実装は、本物と突き合わせた
//! 偽のカレンダーの上で流す）。

use super::google_calendar::repository_tests::repository_on_an_empty_fake_calendar;
use super::in_memory::InMemoryReservationRepository;
use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
use crate::domain::aggregates::reservation::{OccurrenceKey, Reservation};
use crate::domain::common::{EmailAddress, TimePeriod};
use crate::domain::ports::repositories::{
    RepositoryError, ReservationRepository, ReservedSlotQuery,
};
use chrono::{DateTime, Duration, TimeZone, Utc};

const ONLY: OccurrenceKey = OccurrenceKey::Only;

fn holder() -> EmailAddress {
    EmailAddress::new("holder@example.com".to_string()).unwrap()
}

/// 2026年11月16日の`hour`時（UTC）
fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, hour, 0, 0).unwrap()
}

fn period(from: DateTime<Utc>, to: DateTime<Utc>) -> TimePeriod {
    TimePeriod::new(from, to).unwrap()
}

/// `holder`が10時から11時まで押さえる、まだ保存していない単発予約
fn new_reservation_of(holder: &str) -> Reservation {
    Reservation::one_time(
        EmailAddress::new(holder.to_string()).unwrap(),
        period(at(10), at(11)),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100 80GB PCIe".to_string(),
        ))],
        Some("学習".to_string()),
    )
    .unwrap()
}

/// 新しい予約は作られ、版が記録され、保存したとおりに読める
async fn a_new_reservation_is_created_with_a_version<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");

    repository.save(&mut reservation).await.unwrap();

    assert!(reservation.version().is_some(), "保存したあとの版を持つ");
    let stored = repository.find(reservation.id()).await.unwrap().unwrap();
    assert!(
        stored.has_same_content_as(&reservation),
        "保存: {reservation:?}\n読んだ: {stored:?}"
    );
    assert_eq!(stored.version(), reservation.version());
}

/// 読んだあとに誰かが保存した予約は、古い版のまま書き戻せない
async fn saving_over_a_newer_version_is_a_conflict<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");
    repository.save(&mut reservation).await.unwrap();
    let mut stale = repository.find(reservation.id()).await.unwrap().unwrap();
    let mut fresh = stale.clone();
    fresh
        .change_slot_notes(&ONLY, "先に変えた".to_string(), &holder())
        .unwrap();
    repository.save(&mut fresh).await.unwrap();

    stale
        .change_slot_notes(&ONLY, "後から変えた".to_string(), &holder())
        .unwrap();
    let result = repository.save(&mut stale).await;

    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "{result:?}"
    );
    let stored = repository.find(reservation.id()).await.unwrap().unwrap();
    assert_eq!(
        stored.slot(&ONLY).unwrap().notes().map(String::as_str),
        Some("先に変えた"),
        "先に保存した変更が残る"
    );
}

/// 読んだあとに取り消された予約は、書き戻しても作り直されない
async fn a_reservation_cancelled_meanwhile_is_not_recreated<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");
    repository.save(&mut reservation).await.unwrap();
    let mut mine = repository.find(reservation.id()).await.unwrap().unwrap();
    let mut theirs = mine.clone();
    theirs.cancel(&holder()).unwrap();
    repository.save(&mut theirs).await.unwrap();

    mine.change_slot_notes(&ONLY, "編集".to_string(), &holder())
        .unwrap();
    let result = repository.save(&mut mine).await;

    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "{result:?}"
    );
    assert!(repository.find(reservation.id()).await.unwrap().is_none());
}

/// 取り消した予約を保存すると、保存先から無くなる
async fn saving_a_cancelled_reservation_removes_it<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");
    repository.save(&mut reservation).await.unwrap();

    reservation.cancel(&holder()).unwrap();
    repository.save(&mut reservation).await.unwrap();

    assert!(repository.find(reservation.id()).await.unwrap().is_none());
    assert!(
        repository
            .find_overlapping(&period(at(9), at(12)))
            .await
            .unwrap()
            .is_empty()
    );
}

/// 保存する前に取り消した予約は、保存しても何も起きない
async fn a_reservation_cancelled_before_saving_leaves_nothing<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");
    reservation.cancel(&holder()).unwrap();

    repository.save(&mut reservation).await.unwrap();

    assert!(repository.find(reservation.id()).await.unwrap().is_none());
}

/// 使われているIDで新しい予約を作ろうとすると、衝突として断られる
async fn a_new_reservation_with_a_taken_id_is_a_conflict<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut first = new_reservation_of("holder@example.com");
    repository.save(&mut first).await.unwrap();
    let mut second = Reservation::reconstruct(
        first.id().clone(),
        holder(),
        first.resources().to_vec(),
        Some("別の予約".to_string()),
        first.schedule().clone(),
        None,
    )
    .unwrap();

    let result = repository.save(&mut second).await;

    assert!(
        matches!(result, Err(RepositoryError::Conflict)),
        "{result:?}"
    );
    let stored = repository.find(first.id()).await.unwrap().unwrap();
    assert_eq!(
        stored.notes().map(String::as_str),
        Some("学習"),
        "先にあった予約は上書きされない"
    );
}

/// 期間に予約枠が重なる予約を読む。接するだけの予約は含まない
async fn reservations_overlapping_a_period_are_found<R>(repository: &R)
where
    R: ReservationRepository,
{
    let mut reservation = new_reservation_of("holder@example.com");
    repository.save(&mut reservation).await.unwrap();
    let ids = |found: Vec<Reservation>| {
        found
            .iter()
            .map(|r| r.id().as_str().to_string())
            .collect::<Vec<_>>()
    };
    let expected = vec![reservation.id().as_str().to_string()];

    let overlapping = repository
        .find_overlapping(&period(at(10) + Duration::minutes(59), at(12)))
        .await
        .unwrap();
    let right_after = repository
        .find_overlapping(&period(at(11), at(12)))
        .await
        .unwrap();
    let right_before = repository
        .find_overlapping(&period(at(9), at(10)))
        .await
        .unwrap();

    assert_eq!(ids(overlapping), expected);
    assert!(
        ids(right_after).is_empty(),
        "終わる時刻から始まる期間には無い"
    );
    assert!(
        ids(right_before).is_empty(),
        "始まる時刻で終わる期間には無い"
    );
}

/// 予約枠の読み出しは、期間でも予約者でも絞れる
async fn reserved_slots_are_found_by_period_and_by_holder<R>(repository: &R)
where
    R: ReservationRepository + ReservedSlotQuery,
{
    let mut mine = new_reservation_of("holder@example.com");
    repository.save(&mut mine).await.unwrap();
    let mut theirs = Reservation::one_time(
        EmailAddress::new("other@example.com".to_string()).unwrap(),
        period(at(10), at(11)),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            1,
            "A100 80GB PCIe".to_string(),
        ))],
        None,
    )
    .unwrap();
    repository.save(&mut theirs).await.unwrap();
    let morning = period(at(9), at(12));

    let mut all: Vec<String> = repository
        .find_slots_overlapping(&morning)
        .await
        .unwrap()
        .iter()
        .map(|slot| slot.holder_email().as_str().to_string())
        .collect();
    all.sort();
    let held = repository
        .find_slots_held_by(&holder(), &morning)
        .await
        .unwrap();

    assert_eq!(all, vec!["holder@example.com", "other@example.com"]);
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].reservation_id(), mine.id());
    assert_eq!(held[0].time_period(), &period(at(10), at(11)));
}

/// 契約を、どの実装にも確かめるテストにする
macro_rules! contract {
    ($name:ident) => {
        mod $name {
            use super::*;

            #[tokio::test]
            async fn in_memory() {
                super::$name(&InMemoryReservationRepository::new()).await;
            }

            #[tokio::test]
            async fn google_calendar() {
                super::$name(&repository_on_an_empty_fake_calendar()).await;
            }
        }
    };
}

contract!(a_new_reservation_is_created_with_a_version);
contract!(saving_over_a_newer_version_is_a_conflict);
contract!(a_reservation_cancelled_meanwhile_is_not_recreated);
contract!(saving_a_cancelled_reservation_removes_it);
contract!(a_reservation_cancelled_before_saving_leaves_nothing);
contract!(a_new_reservation_with_a_taken_id_is_a_conflict);
contract!(reservations_overlapping_a_period_are_found);
contract!(reserved_slots_are_found_by_period_and_by_holder);
