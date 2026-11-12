use crate::domain::aggregates::reservation::errors::ReservationError;
use crate::domain::common::value_objects::errors::EmailAddressError;
use crate::domain::errors::DomainError;
use crate::domain::ports::error::PortError;
use std::fmt;

/// リポジトリ操作で発生するエラー
#[derive(Debug)]
#[non_exhaustive]
pub enum RepositoryError {
    /// リソースが見つからない
    NotFound,
    /// 接続エラー
    ConnectionError(String),
    /// 無効なメールアドレス
    InvalidEmail(EmailAddressError),
    /// ReservedSlotのドメインルール違反
    InvalidReservation(ReservationError),
    /// 不明なエラー
    Unknown(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepositoryError::NotFound => write!(f, "リソースが見つかりません"),
            RepositoryError::ConnectionError(msg) => write!(f, "接続エラー: {}", msg),
            RepositoryError::InvalidEmail(e) => write!(f, "無効なメールアドレス: {}", e),
            RepositoryError::InvalidReservation(e) => {
                write!(f, "予約のドメインルール違反: {}", e)
            }
            RepositoryError::Unknown(msg) => write!(f, "不明なエラー: {}", msg),
        }
    }
}

impl std::error::Error for RepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepositoryError::InvalidEmail(e) => Some(e),
            RepositoryError::InvalidReservation(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EmailAddressError> for RepositoryError {
    fn from(e: EmailAddressError) -> Self {
        RepositoryError::InvalidEmail(e)
    }
}

impl From<ReservationError> for RepositoryError {
    fn from(e: ReservationError) -> Self {
        RepositoryError::InvalidReservation(e)
    }
}

impl DomainError for RepositoryError {}
impl PortError for RepositoryError {}
