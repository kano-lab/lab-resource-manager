use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use std::sync::Arc;

/// ユーザーのリソース使用予定一覧を取得するユースケース
pub struct ListUserResourceUsagesUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> ListUserResourceUsagesUseCase<R> {
    /// 新しいListUserResourceUsagesUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 特定のユーザーが所有するリソース使用予定の一覧を取得
    ///
    /// # Arguments
    /// * `owner_email` - 所有者のメールアドレス
    ///
    /// # Returns
    /// ResourceUsageのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        owner_email: &EmailAddress,
    ) -> Result<Vec<ResourceUsage>, ApplicationError> {
        let mut usages = self.repository.find_by_owner(owner_email).await?;

        // 開始時刻でソート
        usages.sort_by_key(|a| a.time_period().start());

        Ok(usages)
    }
}
