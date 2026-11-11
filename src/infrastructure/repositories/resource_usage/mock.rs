use crate::domain::{
    aggregates::recurring_reservation::{RecurringReservation, RecurringReservationId},
    aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{TimePeriod, UsageId},
    },
    ports::repositories::{RepositoryError, ResourceUsageRepository},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// テスト用のインメモリResourceUsageリポジトリ実装
#[derive(Clone)]
pub struct MockUsageRepository {
    storage: Arc<Mutex<HashMap<String, ResourceUsage>>>,
    recurring_reservations: Arc<Mutex<HashMap<String, RecurringReservation>>>,
}

impl Default for MockUsageRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MockUsageRepository {
    /// 新しいモックリポジトリを作成
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            recurring_reservations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 定期予約を登録する（各回の`ResourceUsage`は別途`save`する）
    pub fn put_recurring_reservation(&self, reservation: RecurringReservation) {
        self.recurring_reservations
            .lock()
            .unwrap()
            .insert(reservation.id().as_str().to_string(), reservation);
    }

    /// 定期予約を取り除く（各回の`ResourceUsage`は別途`delete`する）
    pub fn remove_recurring_reservation(&self, id: &RecurringReservationId) {
        self.recurring_reservations
            .lock()
            .unwrap()
            .remove(id.as_str());
    }
}

#[async_trait]
impl ResourceUsageRepository for MockUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        Ok(storage.get(id.as_str()).cloned())
    }

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        let overlapping: Vec<ResourceUsage> = storage
            .values()
            .filter(|usage| usage.time_period().overlaps_with(time_period))
            .cloned()
            .collect();
        Ok(overlapping)
    }

    async fn find_by_owner(
        &self,
        owner_email: &crate::domain::common::EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        let owned: Vec<ResourceUsage> = storage
            .values()
            .filter(|usage| usage.owner_email() == owner_email)
            .filter(|usage| usage.time_period().overlaps_with(time_period))
            .cloned()
            .collect();
        Ok(owned)
    }

    async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError> {
        let mut storage = self.storage.lock().unwrap();
        storage.insert(usage.id().as_str().to_string(), usage.clone());
        Ok(())
    }

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError> {
        let mut storage = self.storage.lock().unwrap();
        storage
            .remove(id.as_str())
            .ok_or(RepositoryError::NotFound)?;
        Ok(())
    }

    async fn find_recurring_reservation(
        &self,
        id: &RecurringReservationId,
    ) -> Result<Option<RecurringReservation>, RepositoryError> {
        let reservations = self.recurring_reservations.lock().unwrap();
        Ok(reservations.get(id.as_str()).cloned())
    }
}
