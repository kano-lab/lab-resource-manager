use super::value_objects::ExternalSystem;
use std::fmt;

/// IdentityLink集約のドメインエラー型
#[derive(Debug, Clone, PartialEq)]
pub enum IdentityLinkError {
    /// 指定された外部システムの識別情報が既に存在する
    IdentityAlreadyExists { system: ExternalSystem },
    /// 指定された外部システムの識別情報が見つからない
    IdentityNotFound { system: ExternalSystem },
}

impl fmt::Display for IdentityLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentityAlreadyExists { system } => {
                write!(
                    f,
                    "外部システム {} の識別情報は既に登録されています",
                    system.as_str()
                )
            }
            Self::IdentityNotFound { system } => {
                write!(
                    f,
                    "外部システム {} の識別情報が見つかりません",
                    system.as_str()
                )
            }
        }
    }
}

impl std::error::Error for IdentityLinkError {}

impl crate::domain::errors::DomainError for IdentityLinkError {}
