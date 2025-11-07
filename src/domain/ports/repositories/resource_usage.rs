use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage, ports::repositories::RepositoryError,
};
use async_trait::async_trait;

#[async_trait]
pub trait ResourceUsageRepository {
    async fn find_all(&self) -> Result<Vec<ResourceUsage>, RepositoryError>;
}
