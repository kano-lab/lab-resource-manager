use crate::domain::{
    aggregates::reservation::{Reservation, ReservationId},
    common::TimePeriod,
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// 予約のリポジトリポート（書く側）
///
/// 予約集約を丸ごと読み（`find`）、丸ごと書き戻す（`save`）。予約枠は予約の中の回なので、
/// 予約枠だけを保存する口は無い。
///
/// # 版
/// 読んだ予約は保存先での版を持つ。`save`は、保存先の版が読んだときのままのときだけ
/// 書き、違えば`RepositoryError::Conflict`で断る。読んでから書くまでの間に、誰かが
/// 予約を変えたり取り消したりしていても、それを上書きしたり作り直したりしない。
#[async_trait]
pub trait ReservationRepository {
    /// IDで予約を読む（版つき）
    ///
    /// 取り消された予約は保存先に無いので`None`を返す。
    async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError>;

    /// 予約枠が期間に重なる予約を読む（版つき）
    ///
    /// 予約に起きた変化を監視するために、監視範囲の予約を集約のまま読む。
    async fn find_overlapping(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<Reservation>, RepositoryError>;

    /// 予約を書き戻す
    ///
    /// 予約の状態と版から、することを決める。書いたあとの版を予約に記録する。
    ///
    /// | 状態 | 版 | すること |
    /// | --- | --- | --- |
    /// | 有効 | なし | 作る |
    /// | 有効 | あり | 保存先の版が同じなら書き換える |
    /// | 取り消し済み | あり | 保存先の版が同じなら消す |
    /// | 取り消し済み | なし | 何もしない（保存する前に取り消した） |
    ///
    /// # Errors
    /// 保存先の版が違う、または保存先から消えている場合、`RepositoryError::Conflict`
    async fn save(&self, reservation: &mut Reservation) -> Result<(), RepositoryError>;
}
