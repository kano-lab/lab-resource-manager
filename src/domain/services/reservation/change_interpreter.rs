//! 予約枠の見え方の変化から、予約に起きた出来事を読み取る

use crate::domain::aggregates::reservation::{
    Reservation, ReservationEvent, ReservationId, ReservationKind, ReservedSlot, ReservedSlotId,
};
use crate::domain::ports::repositories::{
    RepositoryError, ReservationRepository, ReservedSlotRepository,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// 予約枠の見え方の変化から、予約に起きた出来事を読み取るドメインサービス
///
/// 監視している期間の予約枠を前回と今回で突き合わせると、現れた・変わった・
/// 見えなくなったという差分が得られる。差分そのものは、予約に何が起きたかを決めない。
/// 何が起きたかを決めるのは予約の知識であり、ここに置く。
///
/// - 予約枠が予定より早く見えなくなったら、取り消されたか、今で終了した。今で終了した
///   予約枠は終了時刻が縮んで監視範囲から外れるだけで、使った分が予約枠として残っている。
///   そこで、見えなくなった予約枠を改めてIDで引いて見分ける
/// - 予約は作る・取り消すの単位なので、出来事も予約を単位とする。定期予約が作られた・
///   変わった・取り消されたは、予約枠がいくつあっても1件の出来事である
/// - 定期予約の予約枠が1つだけ変わったときに限り、その予約枠の出来事とする
/// - 時間が経って定期予約の次の予約枠が監視範囲に入ってきただけなら、何も起きていない
///
/// 予定どおり終わった予約枠も見えなくなるが、何も起きていないので出来事にしない。
#[derive(Debug, Clone, Default)]
pub struct ReservationChangeInterpreter;

/// 前回の監視で分かっていたこと
#[derive(Debug, Clone, Default)]
pub struct WatchedReservations {
    /// 監視範囲に見えていた予約枠
    pub slots: HashMap<ReservedSlotId, ReservedSlot>,
    /// 監視範囲の終わり
    ///
    /// これ以降に始まる予約枠は前回の範囲に入りえなかったので、今回初めて見えても
    /// 新しく予約されたとは限らない（時間の経過で範囲に入ってきただけかもしれない）。
    pub window_end: DateTime<Utc>,
    /// これまでに見た定期予約
    ///
    /// 監視範囲に予約枠が1つも無い時期があっても覚えておく。忘れると、間隔の長い定期予約の
    /// 次の予約枠が範囲に入るたびに、新しい定期予約として伝えてしまう。
    pub recurring_reservations: HashMap<ReservationId, Reservation>,
}

/// 読み取った結果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterpretedChanges {
    /// 予約に起きた出来事
    pub events: Vec<ReservationEvent>,
    /// 新しく見えた予約枠の数
    pub appeared: usize,
    /// 予定より早く見えなくなった予約枠の数
    pub disappeared: usize,
    /// 次の読み取りに引き継ぐ、これまでに見た定期予約
    pub recurring_reservations: HashMap<ReservationId, Reservation>,
}

/// 1件の予約について、前回から今回のあいだに起きた予約枠の変化
struct SlotChanges {
    kind: ReservationKind,
    /// 今回初めて見えた予約枠
    appeared: Vec<ReservedSlot>,
    /// 内容が変わった予約枠
    changed: Vec<ReservedSlot>,
    /// 今で終了して見えなくなった予約枠（終了後の内容）
    ended_early: Vec<ReservedSlot>,
    /// 取り消されて見えなくなった予約枠
    cancelled: Vec<ReservedSlot>,
    /// 今回も見えている予約枠があるか
    still_has_slots: bool,
}

impl SlotChanges {
    /// 予約枠が属する予約の変化の記録を取り出す（無ければ作る）
    fn of<'a>(
        changes: &'a mut HashMap<ReservationId, SlotChanges>,
        slot: &ReservedSlot,
    ) -> &'a mut SlotChanges {
        changes
            .entry(slot.reservation_id())
            .or_insert_with(|| Self {
                kind: slot.reservation_kind(),
                appeared: Vec::new(),
                changed: Vec::new(),
                ended_early: Vec::new(),
                cancelled: Vec::new(),
                still_has_slots: false,
            })
    }

    /// 変化した予約枠の数（時間の経過で範囲に入ってきただけのものを除く）
    fn touched(&self, newly_added: usize) -> usize {
        newly_added + self.changed.len() + self.ended_early.len() + self.cancelled.len()
    }
}

impl ReservationChangeInterpreter {
    /// 新しいインスタンスを作成
    pub fn new() -> Self {
        Self
    }

    /// 見えている予約枠が属する定期予約を引いておく（監視を始めるとき用）
    ///
    /// 監視を始めた時点で既にある定期予約を覚えておかないと、その次の予約枠が範囲に
    /// 入ってきたときに、新しい定期予約として読み取ってしまう。
    ///
    /// # Errors
    /// リポジトリエラー
    pub async fn recurring_reservations_of<R: ReservationRepository + ?Sized>(
        &self,
        repository: &R,
        slots: &HashMap<ReservedSlotId, ReservedSlot>,
    ) -> Result<HashMap<ReservationId, Reservation>, RepositoryError> {
        let mut recurring = HashMap::new();
        for slot in slots.values() {
            let id = slot.reservation_id();
            if slot.reservation_kind() == ReservationKind::Recurring && !recurring.contains_key(&id)
            {
                let reservation = Self::describe_recurring(repository, slot).await?;
                recurring.insert(id, reservation);
            }
        }
        Ok(recurring)
    }

    /// 前回と今回の予約枠から、予約に起きた出来事を読み取る
    ///
    /// # Arguments
    /// * `repository` - 見えなくなった予約枠や、定期予約の今の内容を確かめるためのリポジトリ
    /// * `previous` - 前回の監視で分かっていたこと
    /// * `current` - 今回見えている予約枠
    /// * `now` - 今回の予約枠を読んだ時刻。これより前に終わる予定だった予約枠は、
    ///   予定どおり終わったものとして扱う
    ///
    /// # Errors
    /// リポジトリエラー
    pub async fn interpret<R: ReservedSlotRepository + ReservationRepository + ?Sized>(
        &self,
        repository: &R,
        previous: &WatchedReservations,
        current: &HashMap<ReservedSlotId, ReservedSlot>,
        now: DateTime<Utc>,
    ) -> Result<InterpretedChanges, RepositoryError> {
        let mut by_reservation: HashMap<ReservationId, SlotChanges> = HashMap::new();
        let mut result = InterpretedChanges {
            recurring_reservations: previous.recurring_reservations.clone(),
            ..InterpretedChanges::default()
        };

        for (id, slot) in current {
            let changes = SlotChanges::of(&mut by_reservation, slot);
            changes.still_has_slots = true;
            match previous.slots.get(id) {
                None => {
                    result.appeared += 1;
                    changes.appeared.push(slot.clone());
                }
                Some(previous_slot) if previous_slot != slot => changes.changed.push(slot.clone()),
                Some(_) => {}
            }
        }

        for (id, slot) in &previous.slots {
            if slot.time_period().end() <= now || current.contains_key(id) {
                continue;
            }
            result.disappeared += 1;
            match repository.find_slot(id).await? {
                Some(stored) if stored.time_period().end() <= now => {
                    SlotChanges::of(&mut by_reservation, &stored)
                        .ended_early
                        .push(stored)
                }
                _ => SlotChanges::of(&mut by_reservation, slot)
                    .cancelled
                    .push(slot.clone()),
            }
        }

        for (reservation_id, changes) in by_reservation {
            let event = match changes.kind {
                ReservationKind::OneTime => Self::one_time_event(changes),
                ReservationKind::Recurring => {
                    Self::recurring_event(
                        repository,
                        &reservation_id,
                        changes,
                        previous.window_end,
                        &mut result.recurring_reservations,
                    )
                    .await?
                }
            };
            result.events.extend(event);
        }

        Ok(result)
    }

    /// 単発予約に起きた変化を、出来事にする
    ///
    /// 単発予約は予約枠をちょうど1つ持つので、予約枠の変化がそのまま予約の変化である。
    fn one_time_event(changes: SlotChanges) -> Option<ReservationEvent> {
        let reservation = Reservation::from_one_time_slot;
        if let Some(slot) = changes.appeared.first() {
            Some(ReservationEvent::Reserved(reservation(slot)))
        } else if let Some(slot) = changes.changed.first() {
            Some(ReservationEvent::Changed(reservation(slot)))
        } else if let Some(slot) = changes.ended_early.first() {
            Some(ReservationEvent::EndedEarly(reservation(slot)))
        } else {
            changes
                .cancelled
                .first()
                .map(|slot| ReservationEvent::Cancelled(reservation(slot)))
        }
    }

    /// 定期予約に起きた予約枠の変化を、1件の出来事にまとめる
    ///
    /// 何も起きていなければ`None`を返す。
    async fn recurring_event<R: ReservationRepository + ?Sized>(
        repository: &R,
        id: &ReservationId,
        changes: SlotChanges,
        previous_window_end: DateTime<Utc>,
        known: &mut HashMap<ReservationId, Reservation>,
    ) -> Result<Option<ReservationEvent>, RepositoryError> {
        // 初めて見る定期予約は、予約枠がいくつ見えていても1件の予約として読む
        if !known.contains_key(id) {
            let Some(earliest) = changes
                .appeared
                .iter()
                .min_by_key(|slot| slot.time_period().start())
            else {
                return Ok(None);
            };
            let reservation = Self::describe_recurring(repository, earliest).await?;
            known.insert(id.clone(), reservation.clone());
            return Ok(Some(ReservationEvent::Reserved(reservation)));
        }

        // 前回の監視範囲より先で始まる予約枠は、時間の経過で範囲に入ってきただけである
        let newly_added: Vec<&ReservedSlot> = changes
            .appeared
            .iter()
            .filter(|slot| slot.time_period().start() < previous_window_end)
            .collect();

        let touched = changes.touched(newly_added.len());
        if touched == 0 {
            return Ok(None);
        }

        let current = repository
            .find_reservation(id)
            .await?
            .filter(|reservation| reservation.kind() == ReservationKind::Recurring);

        // 定期予約そのものが無くなり、予約枠も残っていなければ、取り消されたということ
        if current.is_none()
            && !changes.cancelled.is_empty()
            && !changes.still_has_slots
            && let Some(cancelled) = known.remove(id)
        {
            return Ok(Some(ReservationEvent::Cancelled(cancelled)));
        }

        // 予約枠が1つだけ変わったなら、その予約枠の出来事とする
        if touched == 1 {
            let event = if let Some(slot) = changes.changed.first() {
                ReservationEvent::SlotChanged(slot.clone())
            } else if let Some(slot) = changes.ended_early.first() {
                ReservationEvent::SlotEndedEarly(slot.clone())
            } else if let Some(slot) = changes.cancelled.first() {
                ReservationEvent::SlotCancelled(slot.clone())
            } else {
                ReservationEvent::SlotChanged(newly_added[0].clone())
            };
            return Ok(Some(event));
        }

        // 複数の予約枠に及ぶ変化は、定期予約そのものの変更とする
        let Some(reservation) = current.or_else(|| known.get(id).cloned()) else {
            return Ok(None);
        };
        known.insert(id.clone(), reservation.clone());
        Ok(Some(ReservationEvent::Changed(reservation)))
    }

    /// 予約枠が属する定期予約の内容を引く
    ///
    /// 永続化層から引けない場合も、定期予約であること自体は予約枠から分かっている。
    /// 繰り返しの規則は分からないまま、手元の予約枠から分かる範囲で組み立てる。
    async fn describe_recurring<R: ReservationRepository + ?Sized>(
        repository: &R,
        slot: &ReservedSlot,
    ) -> Result<Reservation, RepositoryError> {
        let id = slot.reservation_id();
        Ok(match repository.find_reservation(&id).await? {
            Some(reservation) if reservation.kind() == ReservationKind::Recurring => reservation,
            _ => Reservation::recurring_from_first_slot(id, slot, None),
        })
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
    struct Stored(Vec<ReservedSlot>);

    #[async_trait]
    impl ReservationRepository for Stored {
        async fn add_reservation(&self, _reservation: &Reservation) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn find_reservation(
            &self,
            _id: &ReservationId,
        ) -> Result<Option<Reservation>, RepositoryError> {
            Ok(None)
        }

        async fn remove_reservation(&self, _id: &ReservationId) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    #[async_trait]
    impl ReservedSlotRepository for Stored {
        async fn find_slot(
            &self,
            id: &ReservedSlotId,
        ) -> Result<Option<ReservedSlot>, RepositoryError> {
            Ok(self.0.iter().find(|slot| slot.id() == id).cloned())
        }

        async fn find_slots_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<ReservedSlot>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn find_slots_held_by(
            &self,
            _holder_email: &EmailAddress,
            _time_period: &TimePeriod,
        ) -> Result<Vec<ReservedSlot>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save_slot(&self, _slot: &ReservedSlot) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn remove_slot(&self, _id: &ReservedSlotId) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn slot(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> ReservedSlot {
        ReservedSlot::reconstruct(
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

    fn seen(slots: &[&ReservedSlot]) -> HashMap<ReservedSlotId, ReservedSlot> {
        slots
            .iter()
            .map(|slot| (slot.id().clone(), (*slot).clone()))
            .collect()
    }

    async fn interpret(
        stored: Vec<ReservedSlot>,
        previous: &[&ReservedSlot],
        current: &[&ReservedSlot],
        now: DateTime<Utc>,
    ) -> InterpretedChanges {
        let previous = WatchedReservations {
            slots: seen(previous),
            window_end: now + Duration::days(60),
            recurring_reservations: HashMap::new(),
        };
        ReservationChangeInterpreter::new()
            .interpret(&Stored(stored), &previous, &seen(current), now)
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
