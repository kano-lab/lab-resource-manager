//! 予約の変化から、予約に起きた出来事を読み取る

use crate::domain::aggregates::reservation::{
    Reservation, ReservationEvent, ReservationId, Schedule,
};
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// 予約の変化から、予約に起きた出来事を読み取るドメインサービス
///
/// 監視している期間の予約を前回と今回で突き合わせると、現れた・変わった・
/// 見えなくなったという差分が得られる。差分そのものは、予約に何が起きたかを決めない。
/// 予約は集約のまま突き合わせる。
///
/// - 予約が現れた → 予約された
/// - 予約の内容が変わった → 予約が変わった（単発予約の予約枠が、終わりだけ今より前へ
///   縮んでいれば、今で終了した）
/// - 予約が予定より早く見えなくなった → 取り消されたか、今で終了したか、監視している
///   期間の外へ動かされた
///
/// 最後のものを見分けるのは予約の知識である。今で終了した予約は、終了時刻が
/// 縮んで監視範囲から外れるだけで、使った分が予約枠として残っている。取り消された
/// 予約は残っていない。そこで、見えなくなった予約を改めてIDで引いて確かめる。
///
/// 予定どおり終わった予約も見えなくなるが、何も起きていないので出来事にしない。
/// 版だけが変わった予約も、内容が変わっていないので出来事にしない。
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
                    changes.events.push(Self::change(before, reservation, now));
                }
                Some(_) => {}
            }
        }

        for (id, before) in previous {
            if current.contains_key(id) || Self::last_end(before) <= now {
                continue;
            }
            changes.disappeared += 1;
            let event = match repository.find(id).await? {
                Some(after) => Self::change(before, &after, now),
                None => ReservationEvent::Cancelled(before.clone()),
            };
            changes.events.push(event);
        }

        Ok(changes)
    }

    /// 内容が変わった予約の出来事
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
                was.start() == is.start() && is.end() < was.end() && is.end() <= now
            }
        }
    }

    /// 予約の最後の予約枠が終わる時刻
    fn last_end(reservation: &Reservation) -> DateTime<Utc> {
        match reservation.schedule() {
            Schedule::OneTime(time_period) => time_period.end(),
        }
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
