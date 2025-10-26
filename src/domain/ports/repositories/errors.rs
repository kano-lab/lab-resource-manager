use crate::domain::errors::DomainError;
use crate::domain::ports::error::PortError;
use std::fmt;

#[derive(Debug)]
pub enum RepositoryError {
    NotFound,
    ConnectionError(String),
    Unknown(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepositoryError::NotFound => write!(f, "リソースが見つかりません"),
            RepositoryError::ConnectionError(msg) => write!(f, "接続エラー: {}", msg),
            RepositoryError::Unknown(msg) => write!(f, "不明なエラー: {}", msg),
        }
    }
}

impl std::error::Error for RepositoryError {}
impl DomainError for RepositoryError {}
impl PortError for RepositoryError {}
