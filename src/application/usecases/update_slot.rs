use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::value_objects::{ReservedSlotId, TimePeriod};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::services::{
    AuthorizationPolicy, ReservationAuthorizationPolicy, ResourceConflictChecker,
};
use std::sync::Arc;
use tracing::info;

/// 予約枠を更新するユースケース
pub struct UpdateSlotUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    authorization_policy: ReservationAuthorizationPolicy,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ReservationRepository> UpdateSlotUseCase<R> {
    /// 新しいUpdateSlotUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        let authorization_policy = ReservationAuthorizationPolicy::new();
        let conflict_checker = ResourceConflictChecker::new();
        Self {
            repository,
            authorization_policy,
            conflict_checker,
        }
    }

    /// 予約枠を更新
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    /// * `holder_email` - 予約者のメールアドレス（権限チェック用）
    /// * `new_time_period` - 新しい使用期間（Noneの場合は変更なし）
    /// * `new_notes` - 新しい備考（Noneの場合は変更なし）
    ///
    /// # Returns
    /// 更新成功
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - 予約者が一致しない場合
    /// - 新しい時間枠が競合する場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        holder_email: &EmailAddress,
        new_time_period: Option<TimePeriod>,
        new_notes: Option<String>,
    ) -> Result<(), ApplicationError> {
        // 既存の予約を取得
        let mut slot = self
            .repository
            .find_slot(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        // 認可チェック
        self.authorization_policy
            .authorize_update(holder_email, &slot)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        // 時間枠の更新と競合チェック
        if let Some(new_period) = new_time_period {
            // 競合チェック（自分自身を除外）
            self.conflict_checker
                .check_conflicts(
                    self.repository.as_ref(),
                    &new_period,
                    slot.resources(),
                    Some(slot.id()),
                )
                .await?;

            slot.update_time_period(new_period);
        }

        // 備考の更新
        if let Some(notes) = new_notes {
            slot.update_notes(notes);
        }

        // 更新
        self.repository.save_slot(&slot).await?;

        info!(
            slot_id = %slot.id().as_str(),
            holder = %slot.holder_email().as_str(),
            start = %slot.time_period().start(),
            end = %slot.time_period().end(),
            "reservation updated"
        );

        Ok(())
    }
}
