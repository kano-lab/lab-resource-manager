use crate::domain::common::TimePeriod;
use crate::domain::{
    aggregates::reservation::{Reservation, ReservationId, ReservedSlot, ReservedSlotId},
    common::EmailAddress,
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// 予約と予約枠のリポジトリポート
///
/// # 予約と予約枠
/// 予約は作る・取り消すの単位、予約枠は予約が実際に押さえている時間である。
/// 競合の判定や今で終了は予約枠に対して行うため、予約枠は予約とは別に読み書きする。
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
pub trait ReservationRepository {
    /// IDで予約枠を検索
    async fn find_slot(&self, id: &ReservedSlotId)
    -> Result<Option<ReservedSlot>, RepositoryError>;

    /// 指定期間と重なる予約枠を検索
    ///
    /// 終了済みの予約枠も含めて返す。過去の期間に対する予約（実利用の検知に
    /// もとづく事後予約など）でも競合を検出できる必要があるため、実装は
    /// 「現在時刻より後に終わるもの」で絞り込んではいけない。
    async fn find_slots_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, RepositoryError>;

    /// 指定期間に重なる、特定の予約者の予約枠を検索
    ///
    /// 対象とする期間は呼び出し側が決める（`find_slots_overlapping`と同じ）。
    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, RepositoryError>;

    /// 予約枠を保存する（新規作成または更新）
    ///
    /// 新しい単発予約の予約枠を保存すると、その単発予約が作られる。
    async fn save_slot(&self, slot: &ReservedSlot) -> Result<(), RepositoryError>;

    /// 予約枠を削除する
    ///
    /// 単発予約の予約枠を削除すると、その単発予約も無くなる。
    async fn delete_slot(&self, id: &ReservedSlotId) -> Result<(), RepositoryError>;

    /// IDで予約を検索
    ///
    /// 取り消された予約は`None`を返す。
    async fn find_reservation(
        &self,
        id: &ReservationId,
    ) -> Result<Option<Reservation>, RepositoryError>;

    /// 予約を削除する（その予約枠も無くなる）
    async fn delete_reservation(&self, id: &ReservationId) -> Result<(), RepositoryError>;
}
