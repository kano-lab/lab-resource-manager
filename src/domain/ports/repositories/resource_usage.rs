use crate::domain::{
    aggregates::resource_usage::{
        entity::ResourceUsage,
        value_objects::{TimePeriod, UsageId},
    },
    common::EmailAddress,
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// ResourceUsage集約のリポジトリポート
///
/// # 検索する期間
/// 検索は期間を指定して行う。「今後の予約」のように終わりのない範囲を指定する手段は
/// 用意していない。繰り返し予約は個々の予定へ展開されるため、終わりのない検索は
/// カレンダーが展開できる限りの予定を返してしまう。どこまでを対象とするかは
/// 用途によって異なるため、呼び出し側が決める。
///
/// # 予約として扱う範囲
/// 永続化層のレコードのうち、`ResourceUsage`として解釈できるものだけが予約である。
/// 解釈できないレコードは予約ではなく、このモデルの外にある。検索結果に現れないのは
/// 取りこぼしではなく、予約ではないものを返さないという意味である。
///
/// 永続化層が人の手による編集を受け付ける場合、予約の形式に沿わないレコードは
/// 避けられない。それを予約として復元しようとはしない。
///
/// 予約されないまま使われるリソースは、実利用の観測（`ResourceUsageObserver`）が
/// 扱う領域である。
#[async_trait]
pub trait ResourceUsageRepository {
    /// IDでResourceUsageを検索
    async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError>;

    /// 指定期間と重複するResourceUsageを検索
    ///
    /// 終了済みのリソース使用も含めて返します。過去の期間に対する予約（実利用検知に
    /// もとづく事後予約など）でも競合を検出できる必要があるため、実装は
    /// 「現在時刻より後に終わるもの」で絞り込んではいけません。
    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    /// 指定期間に重なる、特定のユーザーが所有するResourceUsageを検索
    ///
    /// 対象とする期間は呼び出し側が決める（`find_overlapping`と同じ）。
    async fn find_by_owner(
        &self,
        owner_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ResourceUsage>, RepositoryError>;

    /// ResourceUsageを保存（新規作成または更新）
    ///
    /// Domain ID (UUID) を持つResourceUsageを保存します。
    /// マッピングが存在する場合は更新、存在しない場合は新規作成します。
    ///
    /// # Errors
    /// - リポジトリエラー
    async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError>;

    /// ResourceUsageを削除
    async fn delete(&self, id: &UsageId) -> Result<(), RepositoryError>;
}
