use crate::domain::{
    aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{TimePeriod, UsageId},
    },
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

#[async_trait]
pub trait ResourceUsageRepository {
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError>;

    async fn find_all(&self) -> Result<Vec<ResourceUsage>, RepositoryError>;

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError>;

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError>;
}
