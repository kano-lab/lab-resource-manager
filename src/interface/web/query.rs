//! Web画面が必要とする読み取り操作
//!
//! Topcoatの`#[page]`はグローバル関数のため型パラメータを持てず、`app_context`は
//! Rustの型をキーにして値を引く。一方ユースケースはリポジトリ実装でジェネリックなので、
//! そのままでは画面側が参照すべき型を書き表せない。ここでトレイトオブジェクトへ均し、
//! 画面がリポジトリ実装を知らずに済むようにする。

use crate::application::error::ApplicationError;
use crate::application::usecases::list_all_future_resource_usages::ListAllFutureResourceUsagesUseCase;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::ports::repositories::ResourceUsageRepository;
use async_trait::async_trait;
use std::sync::Arc;

/// タイムライン画面が使う問い合わせ
#[async_trait]
pub trait ReservationQuery: Send + Sync {
    /// 指定期間に重なる予約を時系列順で返す
    async fn list_all(&self, window: &TimePeriod) -> Result<Vec<ResourceUsage>, ApplicationError>;
}

/// 既存のユースケースへ委譲する実装
pub struct UseCaseReservationQuery<R: ResourceUsageRepository> {
    list_all: Arc<ListAllFutureResourceUsagesUseCase<R>>,
}

impl<R: ResourceUsageRepository> UseCaseReservationQuery<R> {
    pub fn new(list_all: Arc<ListAllFutureResourceUsagesUseCase<R>>) -> Self {
        Self { list_all }
    }
}

#[async_trait]
impl<R> ReservationQuery for UseCaseReservationQuery<R>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
{
    async fn list_all(&self, window: &TimePeriod) -> Result<Vec<ResourceUsage>, ApplicationError> {
        self.list_all.execute(window).await
    }
}
