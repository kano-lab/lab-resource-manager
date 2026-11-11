use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{ReservedSlot, ReservedSlotId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::services::{AuthorizationPolicy, ReservationAuthorizationPolicy};
use std::sync::Arc;
use tracing::info;

/// 予約枠を取り消すユースケース
///
/// 単発予約の予約枠を取り消すと、その単発予約も無くなる。定期予約の予約枠を取り消しても、
/// 定期予約と他の予約枠は残る。定期予約を丸ごと取り消すのは`CancelReservationUseCase`が担う。
pub struct CancelSlotUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    authorization_policy: ReservationAuthorizationPolicy,
}

impl<R: ReservationRepository> CancelSlotUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            authorization_policy: ReservationAuthorizationPolicy::new(),
        }
    }

    /// 予約枠を取り消す
    ///
    /// # Returns
    /// 取り消した予約枠（どの予約に属していたかを、呼び出し側が伝えられるように）
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    /// * `requested_by` - 操作者のメールアドレス（権限チェック用）
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - 操作者が予約者でない場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        requested_by: &EmailAddress,
    ) -> Result<ReservedSlot, ApplicationError> {
        let slot = self
            .repository
            .find_slot(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        self.authorization_policy
            .authorize_delete(requested_by, &slot)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        self.repository.delete_slot(id).await?;

        info!(
            slot_id = %id.as_str(),
            reservation_id = %slot.reservation_id().as_str(),
            holder = %slot.holder_email().as_str(),
            requested_by = %requested_by.as_str(),
            start = %slot.time_period().start(),
            end = %slot.time_period().end(),
            "reserved slot cancelled"
        );

        Ok(slot)
    }
}
