use std::fmt;

/// EmailAddress Value Objectのエラー型
#[derive(Debug, Clone, PartialEq)]
pub enum EmailAddressError {
    /// '@'が含まれていない
    MissingAtSign,
}

impl fmt::Display for EmailAddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAtSign => write!(f, "無効なメールアドレス形式: '@'が含まれていません"),
        }
    }
}

impl std::error::Error for EmailAddressError {}

impl crate::domain::errors::DomainError for EmailAddressError {}
