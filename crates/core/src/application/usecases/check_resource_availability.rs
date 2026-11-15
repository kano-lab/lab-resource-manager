use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::value_objects::{
    Resource, ResourceCatalog, ResourceSelection,
};
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::ReservedSlotQuery;
use crate::domain::services::reservation::availability::{self, ResourceAvailability};
use std::sync::Arc;

/// 指定期間における各リソースの空きを調べるユースケース
pub struct CheckResourceAvailabilityUseCase<R: ReservedSlotQuery> {
    repository: Arc<R>,
    catalog: Arc<ResourceCatalog>,
}

impl<R: ReservedSlotQuery> CheckResourceAvailabilityUseCase<R> {
    /// 新しいCheckResourceAvailabilityUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    /// * `catalog` - 予約できるリソースの一覧
    pub fn new(repository: Arc<R>, catalog: Arc<ResourceCatalog>) -> Self {
        Self {
            repository,
            catalog,
        }
    }

    /// 予約できるすべてのリソースの空きを取得
    ///
    /// # Arguments
    /// * `window` - 調べる期間。どこまで先を見るかは呼び出し側が決める
    ///
    /// # Returns
    /// リソースの一覧と同じ並びの空き情報
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        window: &TimePeriod,
    ) -> Result<Vec<ResourceAvailability>, ApplicationError> {
        self.availabilities_of(self.catalog.resources(), window)
            .await
    }

    /// 指定のどれかが指すリソースの空きを取得
    ///
    /// # Arguments
    /// * `selections` - 調べるリソースの指定
    /// * `window` - 調べる期間
    ///
    /// # Returns
    /// リソースの一覧と同じ並びの空き情報
    ///
    /// # Errors
    /// - 指定されたリソースが予約できるリソースの一覧に無い場合
    /// - リポジトリエラー
    pub async fn execute_for(
        &self,
        selections: &[ResourceSelection],
        window: &TimePeriod,
    ) -> Result<Vec<ResourceAvailability>, ApplicationError> {
        let resources = self.catalog.select_any(selections)?;
        self.availabilities_of(&resources, window).await
    }

    async fn availabilities_of(
        &self,
        resources: &[Resource],
        window: &TimePeriod,
    ) -> Result<Vec<ResourceAvailability>, ApplicationError> {
        let slots = self.repository.find_slots_overlapping(window).await?;

        Ok(availability::calculate(resources, window, &slots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use crate::testing::InMemoryReservationRepository;
    use chrono::{Duration, Utc};

    fn usecase() -> CheckResourceAvailabilityUseCase<InMemoryReservationRepository> {
        let catalog = ResourceCatalog::new([
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Room {
                name: "会議室A".to_string(),
            },
        ]);
        CheckResourceAvailabilityUseCase::new(
            Arc::new(InMemoryReservationRepository::new()),
            Arc::new(catalog),
        )
    }

    fn window() -> TimePeriod {
        let now = Utc::now();
        TimePeriod::new(now, now + Duration::days(1)).unwrap()
    }

    #[tokio::test]
    async fn without_a_selection_every_resource_in_the_catalog_is_checked() {
        let availabilities = usecase().execute(&window()).await.unwrap();

        assert_eq!(availabilities.len(), 2);
    }

    #[tokio::test]
    async fn a_selection_narrows_the_check_and_an_unknown_one_is_refused() {
        let usecase = usecase();

        let rooms = usecase
            .execute_for(&[ResourceSelection::room("会議室A")], &window())
            .await
            .unwrap();
        let unknown = usecase
            .execute_for(&[ResourceSelection::room("会議室Z")], &window())
            .await;

        assert_eq!(rooms.len(), 1);
        assert!(
            matches!(unknown, Err(ApplicationError::UnknownResource(_))),
            "{unknown:?}"
        );
    }
}
