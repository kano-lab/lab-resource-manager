use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::{entity::ResourceUsage, value_objects::UsageId};
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use std::sync::Arc;

/// IDでリソース使用予定を取得するユースケース
pub struct GetResourceUsageByIdUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> GetResourceUsageByIdUseCase<R> {
    /// 新しいGetResourceUsageByIdUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定されたIDのリソース使用予定を取得
    ///
    /// # Arguments
    /// * `id` - 使用予定ID
    ///
    /// # Returns
    /// ResourceUsage
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - リポジトリエラー
    pub async fn execute(&self, id: &UsageId) -> Result<ResourceUsage, ApplicationError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))
    }
}
