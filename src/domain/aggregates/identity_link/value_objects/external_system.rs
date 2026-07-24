use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// 外部システムの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalSystem {
    /// Slack
    Slack,
    /// GPUサーバーのOSユーザー（実利用検知でのユーザー特定に使用）
    ///
    /// OSユーザー名の名前空間はサーバーごとに異なりうる（NIS/LDAP等で統一されているとは限らない）ため、
    /// サーバー識別子を含めて別システムとして扱う。これにより、同じ利用者がサーバーごとに
    /// 異なるOSユーザー名を持つ場合でも、それぞれを別の`ExternalIdentity`としてリンクできる。
    Os {
        /// サーバー識別子（例: "Thalys"）
        server: String,
    },
}

impl ExternalSystem {
    /// 文字列表現を取得（`IdentityLinkRepository`のファイル永続化等で使用）
    pub fn as_str(&self) -> String {
        match self {
            ExternalSystem::Slack => "slack".to_string(),
            ExternalSystem::Os { server } => format!("os:{}", server),
        }
    }
}

impl FromStr for ExternalSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("slack") {
            return Ok(ExternalSystem::Slack);
        }
        if s.get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("os:"))
        {
            return Ok(ExternalSystem::Os {
                server: s[3..].to_string(),
            });
        }
        Err(format!("Unknown external system: {}", s))
    }
}
