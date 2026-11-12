use crate::domain::read_models::SlotView;
use crate::domain::{
    aggregates::reservation::ReservedSlotId,
    common::{EmailAddress, TimePeriod},
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// 予約枠のリポジトリポート
///
/// # 予約枠を新しく作る手段ではない
/// 予約枠は、予約を加えることで現れる（[`super::ReservationRepository::add_reservation`]）。
/// このリポジトリが保存するのは、既にある予約枠の変更である。
///
/// # 検索する期間
/// 予約枠の検索は期間を指定して行う。「今後の予約枠」のように終わりのない範囲を指定する
/// 手段は用意していない。永続化層が繰り返しの予定を展開する場合、終わりのない検索は
/// 展開できる限りの予約枠を返してしまう。どこまでを対象とするかは
/// 用途によって異なるため、呼び出し側が決める。
///
/// # 予約として扱う範囲
/// 永続化層のレコードのうち、予約枠として解釈できるものだけが予約である。
/// 解釈できないレコードは予約ではなく、このモデルの外にある。検索結果に現れないのは
/// 取りこぼしではなく、予約ではないものを返さないという意味である。
///
/// 永続化層が人の手による編集を受け付ける場合、予約の形式に沿わないレコードは
/// 避けられない。それを予約として復元しようとはしない。
///
/// 予約されないまま使われるリソースは、実利用の観測（`ResourceUsageObserver`）が
/// 扱う領域である。
#[async_trait]
pub trait ReservedSlotQuery {
    /// IDで予約枠を検索
    async fn find_slot(&self, id: &ReservedSlotId) -> Result<Option<SlotView>, RepositoryError>;

    /// 指定期間と重なる予約枠を検索
    ///
    /// 終了済みの予約枠も含めて返す。過去の期間に対する予約（実利用の検知に
    /// もとづく事後予約など）でも競合を検出できる必要があるため、実装は
    /// 「現在時刻より後に終わるもの」で絞り込んではいけない。
    async fn find_slots_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<SlotView>, RepositoryError>;

    /// 指定期間に重なる、特定の予約者の予約枠を検索
    ///
    /// 対象とする期間は呼び出し側が決める（`find_slots_overlapping`と同じ）。
    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<SlotView>, RepositoryError>;

    /// 既にある予約枠の変更を保存する
    ///
    /// # Errors
    /// 予約枠が無い場合、`RepositoryError::NotFound`
    async fn save_slot(&self, slot: &SlotView) -> Result<(), RepositoryError>;

    /// 取り消された予約枠を取り除く
    ///
    /// 単発予約の予約枠を取り除くと、その単発予約も無くなる。
    ///
    /// # Errors
    /// 予約枠が無い場合、`RepositoryError::NotFound`
    async fn remove_slot(&self, id: &ReservedSlotId) -> Result<(), RepositoryError>;
}
