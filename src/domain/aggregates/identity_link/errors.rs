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
                write!(f, "Invalid email format: {}", email)
            }
            Self::AlreadyLinked => {
                write!(f, "This identity is already linked to Slack")
            }
        }
    }
}

impl std::error::Error for IdentityLinkError {}

impl crate::domain::errors::DomainError for IdentityLinkError {}
