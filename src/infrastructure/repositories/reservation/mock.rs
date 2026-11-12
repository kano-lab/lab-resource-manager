use crate::domain::common::TimePeriod;
use crate::domain::{
    aggregates::reservation::{Reservation, ReservedSlot, ReservedSlotId},
    common::EmailAddress,
    ports::repositories::{RepositoryError, ReservationRepository, ReservedSlotRepository},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// テスト用のインメモリ予約・予約枠リポジトリ実装
#[derive(Clone)]
pub struct MockReservationRepository {
    slots: Arc<Mutex<HashMap<String, ReservedSlot>>>,
}

impl Default for MockReservationRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MockReservationRepository {
    /// 新しいモックリポジトリを作成
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 予約枠を、保存先に既にあるものとして置く
    ///
    /// アプリを通さずにカレンダーへ書かれた予約や、テストの前提となる予約枠を用意する。
    /// 同じIDの予約枠があれば置き換える。
    pub fn put_slot(&self, slot: &ReservedSlot) {
        self.slots
            .lock()
            .unwrap()
            .insert(slot.id().as_str().to_string(), slot.clone());
    }
}

#[async_trait]
impl ReservationRepository for MockReservationRepository {
    async fn add_reservation(&self, reservation: &Reservation) -> Result<(), RepositoryError> {
        let slot = reservation
            .one_time_slot()
            .ok_or_else(|| RepositoryError::Unknown("単発予約以外は扱えません".to_string()))?;
        self.put_slot(&slot);
        Ok(())
    }
}

#[async_trait]
impl ReservedSlotRepository for MockReservationRepository {
    async fn find_slot(
        &self,
        id: &ReservedSlotId,
    ) -> Result<Option<ReservedSlot>, RepositoryError> {
        Ok(self.slots.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn find_slots_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .values()
            .filter(|slot| slot.time_period().overlaps_with(time_period))
            .cloned()
            .collect())
    }

    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .values()
            .filter(|slot| slot.holder_email() == holder_email)
            .filter(|slot| slot.time_period().overlaps_with(time_period))
            .cloned()
            .collect())
    }

    async fn save_slot(&self, slot: &ReservedSlot) -> Result<(), RepositoryError> {
        let mut slots = self.slots.lock().unwrap();
        let stored = slots
            .get_mut(slot.id().as_str())
            .ok_or(RepositoryError::NotFound)?;
        *stored = slot.clone();
        Ok(())
    }

    async fn remove_slot(&self, id: &ReservedSlotId) -> Result<(), RepositoryError> {
        self.slots
            .lock()
            .unwrap()
            .remove(id.as_str())
            .ok_or(RepositoryError::NotFound)?;
        Ok(())
    }
}
