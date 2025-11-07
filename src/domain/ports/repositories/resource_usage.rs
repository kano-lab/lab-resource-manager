use crate::domain::{
    aggregates::resource_usage::entity::ResourceUsage, ports::repositories::RepositoryError,
};
use async_trait::async_trait;

#[async_trait]
pub trait ResourceUsageRepository {
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
}
