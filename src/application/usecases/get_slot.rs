use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservedSlotId;
use crate::domain::ports::repositories::{RepositoryError, ReservedSlotQuery};
use crate::domain::read_models::SlotView;
use std::sync::Arc;

/// IDで予約枠を取得するユースケース
pub struct GetSlotUseCase<R: ReservedSlotQuery> {
    repository: Arc<R>,
}

impl<R: ReservedSlotQuery> GetSlotUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定されたIDの予約枠を取得
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - リポジトリエラー
    pub async fn execute(&self, id: &ReservedSlotId) -> Result<SlotView, ApplicationError> {
        self.repository
            .find_slot(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))
    }
}
