//! 予約枠の見え方の変化から、予約に起きた出来事を読み取る

use crate::domain::aggregates::reservation::{Reservation, ReservationEvent, ReservedSlotId};
use crate::domain::ports::repositories::{RepositoryError, ReservedSlotQuery};
use crate::domain::read_models::SlotView;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// 予約枠の見え方の変化から、予約に起きた出来事を読み取るドメインサービス
///
/// 監視している期間の予約枠を前回と今回で突き合わせると、現れた・変わった・
/// 見えなくなったという差分が得られる。差分そのものは、予約に何が起きたかを決めない。
///
/// - 予約枠が現れた → 予約された
/// - 予約枠の内容が変わった → 予約枠が変わった
/// - 予約枠が予定より早く見えなくなった → 取り消されたか、今で終了した
///
/// 最後の2つを見分けるのは予約の知識である。今で終了した予約枠は、終了時刻が
/// 縮んで監視範囲から外れるだけで、使った分が予約枠として残っている。取り消された
/// 予約枠は残っていない。そこで、見えなくなった予約枠を改めてIDで引いて確かめる。
///
/// 予定どおり終わった予約枠も見えなくなるが、何も起きていないので出来事にしない。
#[derive(Debug, Clone, Default)]
pub struct ReservationChangeInterpreter;

/// 読み取った結果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterpretedChanges {
    /// 予約に起きた出来事
    pub events: Vec<ReservationEvent>,
    /// 新しく現れた予約枠の数
    pub appeared: usize,
    /// 予定より早く見えなくなった予約枠の数
    pub disappeared: usize,
}

impl ReservationChangeInterpreter {
    /// 新しいインスタンスを作成
    pub fn new() -> Self {
        Self
    }

    /// 前回と今回の予約枠から、予約に起きた出来事を読み取る
    ///
    /// # Arguments
    /// * `repository` - 見えなくなった予約枠を確かめるための予約枠リポジトリ
    /// * `previous` - 前回見えていた予約枠
    /// * `current` - 今回見えている予約枠
    /// * `now` - 今回の予約枠を読んだ時刻。これより前に終わる予定だった予約枠は、
    ///   予定どおり終わったものとして扱う
    ///
    /// # Errors
    /// 見えなくなった予約枠の確認でリポジトリエラーが起きた場合
    pub async fn interpret<R: ReservedSlotQuery + ?Sized>(
        &self,
        repository: &R,
        previous: &HashMap<ReservedSlotId, SlotView>,
        current: &HashMap<ReservedSlotId, SlotView>,
        now: DateTime<Utc>,
    ) -> Result<InterpretedChanges, RepositoryError> {
        let mut changes = InterpretedChanges::default();

        for (id, slot) in current {
            match previous.get(id) {
                None => {
                    changes.appeared += 1;
                    changes.events.push(ReservationEvent::Reserved(
                        Reservation::from_one_time_slot(slot),
                    ));
                }
                Some(previous_slot) if previous_slot != slot => {
                    changes
                        .events
                        .push(ReservationEvent::Changed(Reservation::from_one_time_slot(
                            slot,
                        )))
                }
                Some(_) => {}
            }
        }

        for (id, slot) in previous {
            if slot.time_period().end() <= now || current.contains_key(id) {
                continue;
            }
            changes.disappeared += 1;
            let event = match repository.find_slot(id).await? {
                Some(stored) if stored.time_period().end() <= now => {
                    ReservationEvent::EndedEarly(Reservation::from_one_time_slot(&stored))
                }
                _ => ReservationEvent::Cancelled(Reservation::from_one_time_slot(slot)),
            };
            changes.events.push(event);
        }

        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::common::{EmailAddress, TimePeriod};
    use async_trait::async_trait;
    use chrono::Duration;

    /// 見えなくなった予約枠を確かめるときに、保存先に残っている予約枠だけを返すスタブ
    struct Stored(Vec<SlotView>);

    #[async_trait]
    impl ReservedSlotQuery for Stored {
        async fn find_slot(
            &self,
            id: &ReservedSlotId,
        ) -> Result<Option<SlotView>, RepositoryError> {
            Ok(self.0.iter().find(|slot| slot.id() == id).cloned())
        }

        async fn find_slots_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<SlotView>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn find_slots_held_by(
            &self,
            _holder_email: &EmailAddress,
            _time_period: &TimePeriod,
        ) -> Result<Vec<SlotView>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save_slot(&self, _slot: &SlotView) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn remove_slot(&self, _id: &ReservedSlotId) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn slot(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> SlotView {
        SlotView::reconstruct(
            ReservedSlotId::from_string(id.to_string()),
            holder(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    fn seen(slots: &[&SlotView]) -> HashMap<ReservedSlotId, SlotView> {
        slots
            .iter()
            .map(|slot| (slot.id().clone(), (*slot).clone()))
            .collect()
    }

    async fn interpret(
        stored: Vec<SlotView>,
        previous: &[&SlotView],
        current: &[&SlotView],
        now: DateTime<Utc>,
    ) -> InterpretedChanges {
        ReservationChangeInterpreter::new()
            .interpret(&Stored(stored), &seen(previous), &seen(current), now)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_slot_that_appears_is_a_new_reservation() {
        let now = Utc::now();
        let new = slot("new", now + Duration::hours(1), now + Duration::hours(2));

        let changes = interpret(vec![], &[], &[&new], now).await;

        assert_eq!(changes.appeared, 1);
        assert_eq!(
            changes.events,
            vec![ReservationEvent::Reserved(Reservation::from_one_time_slot(
                &new
            ))]
        );
    }

    #[tokio::test]
    async fn a_slot_whose_contents_differ_has_changed() {
        let now = Utc::now();
        let before = slot("slot", now + Duration::hours(1), now + Duration::hours(2));
        let after = slot("slot", now + Duration::hours(1), now + Duration::hours(3));

        let changes = interpret(vec![], &[&before], &[&after], now).await;

        assert_eq!(
            changes.events,
            vec![ReservationEvent::Changed(Reservation::from_one_time_slot(
                &after
            ))]
        );
    }

    #[tokio::test]
    async fn a_slot_that_vanished_but_is_still_stored_with_an_earlier_end_ended_early() {
        let now = Utc::now();
        let running = slot("slot", now - Duration::hours(1), now + Duration::hours(2));
        let ended = slot("slot", now - Duration::hours(1), now - Duration::minutes(1));

        let changes = interpret(vec![ended.clone()], &[&running], &[], now).await;

        assert_eq!(changes.disappeared, 1);
        assert_eq!(
            changes.events,
            vec![ReservationEvent::EndedEarly(
                Reservation::from_one_time_slot(&ended)
            )],
            "使った分が予約枠として残っているなら、取り消しではない"
        );
    }

    #[tokio::test]
    async fn a_slot_that_vanished_and_is_no_longer_stored_was_cancelled() {
        let now = Utc::now();
        let upcoming = slot("slot", now + Duration::hours(1), now + Duration::hours(2));

        let changes = interpret(vec![], &[&upcoming], &[], now).await;

        assert_eq!(
            changes.events,
            vec![ReservationEvent::Cancelled(
                Reservation::from_one_time_slot(&upcoming)
            )]
        );
    }

    #[tokio::test]
    async fn a_slot_that_ran_its_course_is_not_an_event() {
        let now = Utc::now();
        let finished = slot("slot", now - Duration::hours(2), now - Duration::minutes(1));

        let changes = interpret(vec![finished.clone()], &[&finished], &[], now).await;

        assert_eq!(changes, InterpretedChanges::default());
    }
}
