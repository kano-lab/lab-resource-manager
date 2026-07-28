use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::ports::repositories::ResourceUsageRepository;
use std::sync::Arc;

/// 指定期間のリソース使用予定を一覧するユースケース
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

    /// 指定期間に重なるリソース使用予定を取得
    ///
    /// # Arguments
    /// * `time_period` - 一覧する期間。どこまで先を見るかは呼び出し側が決める
    ///
    /// # Returns
    /// ResourceUsageのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, ApplicationError> {
        let mut usages = self.repository.find_overlapping(time_period).await?;

        // 開始時刻でソート
        usages.sort_by_key(|a| a.time_period().start());

        Ok(usages)
    }
}
