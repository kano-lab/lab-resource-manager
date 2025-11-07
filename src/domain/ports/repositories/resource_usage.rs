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

    /// 未来のリソース使用状況を取得する（進行中および今後予定されているもの）
    ///
    /// このメソッドは、終了時刻が現在時刻より後のリソース使用状況を返します。
    /// 過去に終了したリソース使用は含まれません。
    ///
    /// # Returns
    /// 進行中および未来のリソース使用状況のリスト
    ///
    /// (Get future resource usages - ongoing and upcoming)
    async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError>;

    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError>;

    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError>;
}
