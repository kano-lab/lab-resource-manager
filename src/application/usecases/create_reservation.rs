use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{
    ReservationId, ReservedSlot,
    value_objects::{Resource, TimePeriod},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ReservationRepository;
use crate::domain::services::ResourceConflictChecker;
use std::sync::Arc;
use tracing::info;

/// 単発予約を作るユースケース
pub struct CreateReservationUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ReservationRepository> CreateReservationUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            conflict_checker: ResourceConflictChecker::new(),
        }
    }

    /// 単発予約を作る
    ///
    /// # Arguments
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 期間
    /// * `resources` - 押さえるリソース
    /// * `notes` - 備考（オプション）
    ///
    /// # Returns
    /// 作られた単発予約のID（その予約枠のIDと同じ）
    ///
    /// # Errors
    /// - 指定期間に、同じリソースを押さえる予約枠がある場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<ReservationId, ApplicationError> {
        self.conflict_checker
            .check_conflicts(self.repository.as_ref(), &time_period, &resources, None)
            .await?;

        let slot = ReservedSlot::new(holder_email, time_period, resources, notes)?;
        self.repository.save_slot(&slot).await?;

        info!(
            reservation_id = %slot.reservation_id().as_str(),
            holder = %slot.holder_email().as_str(),
            resources = %slot
                .resources()
                .iter()
                .map(|resource| resource.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            start = %slot.time_period().start(),
            end = %slot.time_period().end(),
            "reservation created"
        );

        Ok(slot.reservation_id().clone())
    }
}
