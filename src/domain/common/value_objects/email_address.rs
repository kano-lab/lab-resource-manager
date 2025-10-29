use super::errors::EmailAddressError;
use serde::{Deserialize, Serialize};

/// メールアドレスを表すValue Object
///
/// 複数の集約で共有される汎用的な概念として、Shared Kernelに配置されている。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmailAddress(String);

impl EmailAddress {
    /// 新しいメールアドレスを作成
    ///
    /// # エラー
    /// - '@'が含まれていない場合
    pub fn new(email: String) -> Result<Self, EmailAddressError> {
        if email.contains('@') {
            Ok(Self(email))
        } else {
            Err(EmailAddressError::MissingAtSign)
        }
    }

    /// '@'より前の部分（ローカルパート）を取得
    /// 例: "user@example.com" -> "user"
    pub fn local_part(&self) -> &str {
        self.0.split('@').next().unwrap_or(&self.0)
    }

    /// 完全なメールアドレス文字列を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_address_valid() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        assert_eq!(email.as_str(), "user@example.com");
        assert_eq!(email.local_part(), "user");
    }

    #[test]
    fn test_email_address_invalid() {
        let result = EmailAddress::new("invalid-email".to_string());
        assert!(result.is_err());
    }
}
