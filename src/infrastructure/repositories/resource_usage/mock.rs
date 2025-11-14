use crate::domain::{
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
        }
    }
}

#[async_trait]
impl ResourceUsageRepository for MockUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        Ok(storage.get(id.as_str()).cloned())
    }

    async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        Ok(storage.values().cloned().collect())
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
    ) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        let owned: Vec<ResourceUsage> = storage
            .values()
            .filter(|usage| usage.owner_email() == owner_email)
            .cloned()
            .collect();
        Ok(owned)
    }

    async fn create(&self, usage: &ResourceUsage) -> Result<UsageId, RepositoryError> {
        let mut storage = self.storage.lock().unwrap();

        // テスト用の簡易的なID生成（タイムスタンプベース）
        let generated_id = if usage.id().as_str().is_empty() {
            use std::time::{SystemTime, UNIX_EPOCH};
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            UsageId::new(format!("mock-{}", timestamp))
        } else {
            usage.id().clone()
        };

        storage.insert(generated_id.as_str().to_string(), usage.clone());
        Ok(generated_id)
    }

    async fn update(&self, usage: &ResourceUsage) -> Result<(), RepositoryError> {
        let mut storage = self.storage.lock().unwrap();
        let id = usage.id().as_str();

        if id.is_empty() {
            return Err(RepositoryError::Unknown("更新にはIDが必要です".to_string()));
        }

        if !storage.contains_key(id) {
            return Err(RepositoryError::NotFound);
        }

        storage.insert(id.to_string(), usage.clone());
        Ok(())
    }

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError> {
        let mut storage = self.storage.lock().unwrap();
        storage
            .remove(id.as_str())
            .ok_or(RepositoryError::NotFound)?;
        Ok(())
    }
}
