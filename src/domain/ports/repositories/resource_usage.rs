use crate::domain::{
    aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{TimePeriod, UsageId},
    },
    common::EmailAddress,
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// ResourceUsage集約のリポジトリポート
#[async_trait]
pub trait ResourceUsageRepository {
    /// IDでResourceUsageを検索
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError>;

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

    /// 指定期間と重複するResourceUsageを検索
    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    /// 特定のユーザーが所有するResourceUsageを検索
    async fn find_by_owner(
        &self,
        owner_email: &EmailAddress,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    /// ResourceUsageを保存
    ///
    /// # Returns
    /// 保存されたResourceUsageのID（新規作成時はAPIが生成したID、更新時は既存のID）
    async fn save(&self, usage: &ResourceUsage) -> Result<UsageId, RepositoryError>;

    /// ResourceUsageを削除
    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError>;
}
