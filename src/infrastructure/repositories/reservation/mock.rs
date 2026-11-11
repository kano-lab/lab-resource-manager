use crate::domain::{
    aggregates::reservation::{
        OneTimeReservation, RecurringReservation, Reservation, ReservationId, ReservationKind,
        ReservedSlot, ReservedSlotId, value_objects::TimePeriod,
    },
    common::EmailAddress,
    ports::repositories::{RepositoryError, ReservationRepository},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// テスト用のインメモリ予約リポジトリ実装
#[derive(Clone)]
pub struct MockReservationRepository {
    slots: Arc<Mutex<HashMap<String, ReservedSlot>>>,
    recurring_reservations: Arc<Mutex<HashMap<String, RecurringReservation>>>,
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
            recurring_reservations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 定期予約を登録する（予約枠は別途`save_slot`する）
    pub fn put_recurring_reservation(&self, reservation: RecurringReservation) {
        self.recurring_reservations
            .lock()
            .unwrap()
            .insert(reservation.id().as_str().to_string(), reservation);
    }

    /// 定期予約の定義だけを取り除く（予約枠は別途`delete_slot`する）
    pub fn remove_recurring_reservation(&self, id: &ReservationId) {
        self.recurring_reservations
            .lock()
            .unwrap()
            .remove(id.as_str());
    }
}

#[async_trait]
impl ReservationRepository for MockReservationRepository {
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
        self.slots
            .lock()
            .unwrap()
            .insert(slot.id().as_str().to_string(), slot.clone());
        Ok(())
    }

    async fn delete_slot(&self, id: &ReservedSlotId) -> Result<(), RepositoryError> {
        self.slots
            .lock()
            .unwrap()
            .remove(id.as_str())
            .ok_or(RepositoryError::NotFound)?;
        Ok(())
    }

    async fn find_reservation(
        &self,
        id: &ReservationId,
    ) -> Result<Option<Reservation>, RepositoryError> {
        if let Some(recurring) = self.recurring_reservations.lock().unwrap().get(id.as_str()) {
            return Ok(Some(Reservation::Recurring(recurring.clone())));
        }

        Ok(self
            .slots
            .lock()
            .unwrap()
            .values()
            .find(|slot| {
                slot.reservation_kind() == ReservationKind::OneTime && slot.reservation_id() == id
            })
            .map(|slot| Reservation::OneTime(OneTimeReservation::from_slot(slot))))
    }

    async fn delete_reservation(&self, id: &ReservationId) -> Result<(), RepositoryError> {
        let removed_definition = self
            .recurring_reservations
            .lock()
            .unwrap()
            .remove(id.as_str())
            .is_some();

        let mut slots = self.slots.lock().unwrap();
        let before = slots.len();
        slots.retain(|_, slot| slot.reservation_id() != id);
        let removed_slots = slots.len() < before;

        if removed_definition || removed_slots {
            Ok(())
        } else {
            Err(RepositoryError::NotFound)
        }
    }
}
