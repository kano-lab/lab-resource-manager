use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage,
    ports::repositories::{RepositoryError, ResourceUsageRepository},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ResourceUsageRepository for MockUsageRepository {
    async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
        let storage = self.storage.lock().unwrap();
        Ok(storage.values().cloned().collect())
    }
}
