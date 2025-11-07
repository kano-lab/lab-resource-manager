use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage, ports::repositories::RepositoryError,
};
use async_trait::async_trait;

#[async_trait]
pub trait ResourceUsageRepository {
    /// 進行中または今後予定されているリソース使用状況を取得する
    /// (Get ongoing or upcoming resource usages)
    async fn find_active(&self) -> Result<Vec<ResourceUsage>, RepositoryError>;
}
