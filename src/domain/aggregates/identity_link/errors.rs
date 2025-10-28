use std::fmt;

/// IdentityLink集約のドメインエラー型
#[derive(Debug, Clone, PartialEq)]
pub enum IdentityLinkError {
    /// 無効なメールアドレス形式
    InvalidEmailFormat(String),
    /// 既にSlackと紐付け済み
    AlreadyLinked,
}

impl fmt::Display for IdentityLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEmailFormat(email) => {
                write!(f, "無効なメールアドレス形式: {}", email)
            }
            Self::AlreadyLinked => {
                write!(f, "このIDは既にSlackと紐付けられています")
            }
        }
    }
}

impl std::error::Error for IdentityLinkError {}

impl crate::domain::errors::DomainError for IdentityLinkError {}
