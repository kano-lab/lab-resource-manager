// NOTE: これ以上肥大化するようであればnotifierディレクトリを作成してその中に適宜分割する
use crate::domain::{
    aggregates::reservation::ReservationEvent, errors::DomainError, ports::PortError,
};
use async_trait::async_trait;
use std::fmt;

/// 予約に起きた出来事を予約チャンネルへ知らせるポート
///
/// 何をどの言葉で知らせるか（テンプレート、宛先、ボタン）は実装が決める。
#[async_trait]
pub trait Notifier: Send + Sync {
    /// 出来事を知らせる
    ///
    /// 知らせきれなかったときはエラーを返す。呼び出し側は、あとで同じ出来事をもう一度渡して
    /// 送り直す。実装は、同じ出来事をすでに届けた送り先へ重ねて届けない。
    async fn notify(&self, event: ReservationEvent) -> Result<(), NotificationError>;
}

/// 通知エラー
#[derive(Debug)]
#[non_exhaustive]
pub enum NotificationError {
    /// 通知送信の失敗
    SendFailure(String),
    /// 宛先が分からない（IdentityLink未登録、Slackアカウント未リンク等）
    ///
    /// 送る手段の不調ではなく、送り先を知らないという状態。再試行しても届くように
    /// ならないため、呼び出し側は送信失敗とは別の扱いを選べる。
    RecipientUnknown(String),
    /// リポジトリエラー（IdentityLink取得失敗等）
    RepositoryError(String),
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationError::SendFailure(msg) => write!(f, "通知送信エラー: {}", msg),
            NotificationError::RecipientUnknown(msg) => {
                write!(f, "通知の宛先が分かりません: {}", msg)
            }
            NotificationError::RepositoryError(msg) => {
                write!(f, "通知準備中のリポジトリエラー: {}", msg)
            }
        }
    }
}

impl std::error::Error for NotificationError {}
impl DomainError for NotificationError {}
impl PortError for NotificationError {}
