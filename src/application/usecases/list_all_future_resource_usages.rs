use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::repositories::ResourceUsageRepository;
use std::sync::Arc;

/// 全ての未来のリソース使用予定を取得するユースケース
pub struct ListAllFutureResourceUsagesUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> ListAllFutureResourceUsagesUseCase<R> {
    /// 新しいListAllFutureResourceUsagesUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 全ての未来のリソース使用予定を取得
    ///
    /// # Returns
    /// ResourceUsageのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(&self) -> Result<Vec<ResourceUsage>, ApplicationError> {
        let mut usages = self.repository.find_future().await?;

        // 開始時刻でソート
        usages.sort_by_key(|a| a.time_period().start());

        Ok(usages)
    }
}
