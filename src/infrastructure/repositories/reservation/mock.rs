use crate::domain::{
    aggregates::reservation::{Reservation, ReservationId, ReservationStatus, Schedule, Version},
    common::{EmailAddress, TimePeriod},
    ports::repositories::{RepositoryError, ReservationRepository, ReservedSlotQuery},
    read_models::ReservedSlotView,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// テスト用のインメモリ予約リポジトリ実装
///
/// 予約を版つきで持つ。版は保存のたびに1ずつ進む。
#[derive(Clone, Default)]
pub struct MockReservationRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    reservations: HashMap<String, Reservation>,
    next_version: u64,
}

impl State {
    fn next_version(&mut self) -> Version {
        self.next_version += 1;
        Version::new(format!("\"{}\"", self.next_version))
    }

    /// 保存先に置く（版を新しくする）
    fn store(&mut self, mut reservation: Reservation) -> Version {
        let version = self.next_version();
        reservation.record_version(Some(version.clone()));
        self.reservations
            .insert(reservation.id().as_str().to_string(), reservation);
        version
    }
}

impl MockReservationRepository {
    /// 新しいモックリポジトリを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 予約を、保存先に既にあるものとして置く
    ///
    /// アプリを通さずにカレンダーへ書かれた予約や、テストの前提となる予約を用意する。
    /// 同じIDの予約があれば置き換え、版を新しくする。
    pub fn put_reservation(&self, reservation: &Reservation) {
        self.state.lock().unwrap().store(reservation.clone());
    }

    /// 予約枠の写しから単発予約を組み立て、保存先に既にあるものとして置く
    pub fn put_slot(&self, slot: &ReservedSlotView) {
        let reservation = Reservation::reconstruct(
            slot.reservation_id().clone(),
            slot.holder_email().clone(),
            slot.resources().to_vec(),
            slot.notes().cloned(),
            Schedule::OneTime(slot.time_period().clone()),
            None,
        )
        .expect("予約枠の写しはリソースを持つ");
        self.put_reservation(&reservation);
    }

    /// 保存先から予約を消す（カレンダー上で直接消されたことを表す）
    pub fn delete_reservation(&self, id: &ReservationId) {
        self.state.lock().unwrap().reservations.remove(id.as_str());
    }

    /// 予約枠IDで予約枠の写しを引く（テストで保存先の様子を確かめる用）
    ///
    /// 予約枠を読む口は、本番では予約を読んでその中の予約枠を見る。テストでは
    /// 結果を確かめたいだけなので、その手順をまとめて提供する。
    pub async fn find_slot(
        &self,
        id: &crate::domain::aggregates::reservation::ReservedSlotId,
    ) -> Result<Option<ReservedSlotView>, RepositoryError> {
        Ok(self
            .find(id.reservation_id())
            .await?
            .and_then(|reservation| {
                reservation
                    .slot(id.key())
                    .map(|slot| ReservedSlotView::of(&reservation, &slot))
            }))
    }

    fn views(&self, time_period: &TimePeriod) -> Vec<ReservedSlotView> {
        self.state
            .lock()
            .unwrap()
            .reservations
            .values()
            .flat_map(|reservation| {
                reservation
                    .slots_in(time_period)
                    .into_iter()
                    .map(|slot| ReservedSlotView::of(reservation, &slot))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

#[async_trait]
impl ReservationRepository for MockReservationRepository {
    async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .reservations
            .get(id.as_str())
            .cloned())
    }

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<Reservation>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .reservations
            .values()
            .filter(|reservation| !reservation.slots_in(time_period).is_empty())
            .cloned()
            .collect())
    }

    async fn save(&self, reservation: &mut Reservation) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        let key = reservation.id().as_str().to_string();
        let stored_version = state
            .reservations
            .get(&key)
            .and_then(|stored| stored.version().cloned());

        match (reservation.status(), reservation.version()) {
            (ReservationStatus::Active, None) => {
                if stored_version.is_some() {
                    return Err(RepositoryError::Conflict);
                }
                let version = state.store(reservation.clone());
                reservation.record_version(Some(version));
            }
            (ReservationStatus::Active, Some(version)) => {
                if stored_version.as_ref() != Some(version) {
                    return Err(RepositoryError::Conflict);
                }
                let version = state.store(reservation.clone());
                reservation.record_version(Some(version));
            }
            (ReservationStatus::Cancelled, Some(version)) => {
                if stored_version.as_ref() != Some(version) {
                    return Err(RepositoryError::Conflict);
                }
                state.reservations.remove(&key);
            }
            (ReservationStatus::Cancelled, None) => {}
        }
        Ok(())
    }
}

#[async_trait]
impl ReservedSlotQuery for MockReservationRepository {
    async fn find_slots_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
        Ok(self.views(time_period))
    }

    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
        Ok(self
            .views(time_period)
            .into_iter()
            .filter(|slot| slot.holder_email() == holder_email)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::OccurrenceKey;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};

    const ONLY: OccurrenceKey = OccurrenceKey::Only;
    use chrono::{Duration, Utc};

    fn holder() -> EmailAddress {
        EmailAddress::new("holder@example.com".to_string()).unwrap()
    }

    fn new_reservation() -> Reservation {
        let start = Utc::now() + Duration::hours(1);
        Reservation::one_time(
            holder(),
            TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn saving_a_new_reservation_creates_it_and_records_its_version() {
        let repository = MockReservationRepository::new();
        let mut reservation = new_reservation();

        repository.save(&mut reservation).await.unwrap();

        assert!(reservation.version().is_some());
        let stored = repository.find(reservation.id()).await.unwrap().unwrap();
        assert_eq!(stored.version(), reservation.version());
    }

    #[tokio::test]
    async fn saving_over_a_newer_version_is_refused() {
        let repository = MockReservationRepository::new();
        let mut reservation = new_reservation();
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
    }

    #[tokio::test]
    async fn saving_a_reservation_that_was_deleted_meanwhile_does_not_recreate_it() {
        let repository = MockReservationRepository::new();
        let mut reservation = new_reservation();
        repository.save(&mut reservation).await.unwrap();
        let mut loaded = repository.find(reservation.id()).await.unwrap().unwrap();
        repository.delete_reservation(reservation.id());

        loaded
            .change_slot_notes(&ONLY, "編集".to_string(), &holder())
            .unwrap();
        let result = repository.save(&mut loaded).await;

        assert!(
            matches!(result, Err(RepositoryError::Conflict)),
            "{result:?}"
        );
        assert!(repository.find(reservation.id()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn saving_a_cancelled_reservation_removes_it() {
        let repository = MockReservationRepository::new();
        let mut reservation = new_reservation();
        repository.save(&mut reservation).await.unwrap();

        reservation.cancel(&holder()).unwrap();
        repository.save(&mut reservation).await.unwrap();

        assert!(repository.find(reservation.id()).await.unwrap().is_none());
    }
}
