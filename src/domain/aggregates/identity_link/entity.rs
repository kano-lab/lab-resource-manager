use super::errors::IdentityLinkError;
use super::value_objects::{EmailAddress, SlackUserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// メールアドレスと Slack の紐付け情報を管理する集約ルート
///
/// この集約は以下の責務を持つ:
/// - メールアドレスをシステム内のユーザー識別子として管理
/// - Slackユーザーとの紐付け状態を管理
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IdentityLink {
    /// 未リンク状態：メールアドレスのみが登録された状態
    ///
    /// Googleカレンダーからイベント作成者情報を取得した際にこの状態で作成される
    Unlinked {
        email: EmailAddress,
        created_at: DateTime<Utc>,
    },
    /// リンク済み状態：Slackユーザーと紐付け済みの状態
    ///
    /// ユーザーが/register-calendarコマンドを実行するか、
    /// 管理者が/link-userコマンドを実行した際にこの状態になる
    Linked {
        email: EmailAddress,
        slack_user_id: SlackUserId,
        created_at: DateTime<Utc>,
        slack_linked_at: DateTime<Utc>,
    },
}

impl IdentityLink {
    /// メールアドレス情報のみで新規作成（未リンク状態）
    ///
    /// Googleカレンダーからイベント作成者情報を取得した際に使用
    pub fn create_from_email(email: EmailAddress) -> Self {
        Self::Unlinked {
            email,
            created_at: Utc::now(),
        }
    }

    /// Slackユーザー登録時の作成（リンク済み状態）
    ///
    /// ユーザー登録時に直接Linked状態で作成する
    pub fn create_with_slack(email: EmailAddress, slack_user_id: SlackUserId) -> Self {
        let now = Utc::now();
        Self::Linked {
            email,
            slack_user_id,
            created_at: now,
            slack_linked_at: now,
        }
    }

    /// 永続化層からの復元
    ///
    /// **Repository実装専用**。保存されていた状態をそのまま復元する。
    /// ビジネスロジックは適用されない（時刻は指定された値がそのまま使われる）。
    ///
    /// 通常のビジネスロジックでEntityを生成する場合は、
    /// [`create_from_email`](Self::create_from_email) または
    /// [`create_with_slack`](Self::create_with_slack) を使用すること。
    ///
    /// # 引数
    /// * `email` - メールアドレス
    /// * `slack_user_id` - Slackユーザーid（Noneの場合はUnlinked状態）
    /// * `created_at` - 作成日時
    /// * `slack_linked_at` - Slack紐付け日時（Linkedの場合は必須）
    pub(crate) fn reconstitute(
        email: EmailAddress,
        slack_user_id: Option<SlackUserId>,
        created_at: DateTime<Utc>,
        slack_linked_at: Option<DateTime<Utc>>,
    ) -> Self {
        match (slack_user_id, slack_linked_at) {
            (Some(sid), Some(sat)) => Self::Linked {
                email,
                slack_user_id: sid,
                created_at,
                slack_linked_at: sat,
            },
            _ => Self::Unlinked { email, created_at },
        }
    }

    /// Slackユーザーを紐付け（状態遷移）
    ///
    /// Unlinked → Linked への状態遷移を行う。
    /// 新しいインスタンスを返すため、元のインスタンスは消費される。
    ///
    /// # エラー
    /// - 既にSlackと紐付け済みの場合
    pub fn link_slack(self, slack_user_id: SlackUserId) -> Result<Self, IdentityLinkError> {
        match self {
            Self::Unlinked { email, created_at } => Ok(Self::Linked {
                email,
                slack_user_id,
                created_at,
                slack_linked_at: Utc::now(),
            }),
            Self::Linked { .. } => Err(IdentityLinkError::AlreadyLinked),
        }
    }

    /// Slack登録済みか判定
    pub fn is_slack_linked(&self) -> bool {
        matches!(self, Self::Linked { .. })
    }

    pub fn email(&self) -> &EmailAddress {
        match self {
            Self::Unlinked { email, .. } => email,
            Self::Linked { email, .. } => email,
        }
    }

    pub fn slack_user_id(&self) -> Option<&SlackUserId> {
        match self {
            Self::Unlinked { .. } => None,
            Self::Linked { slack_user_id, .. } => Some(slack_user_id),
        }
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        match self {
            Self::Unlinked { created_at, .. } => *created_at,
            Self::Linked { created_at, .. } => *created_at,
        }
    }

    pub fn slack_linked_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Unlinked { .. } => None,
            Self::Linked {
                slack_linked_at, ..
            } => Some(*slack_linked_at),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_from_email() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let identity = IdentityLink::create_from_email(email.clone());

        assert_eq!(identity.email(), &email);
        assert!(!identity.is_slack_linked());
        assert!(identity.slack_user_id().is_none());
    }

    #[test]
    fn test_create_with_slack() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let slack_id = SlackUserId::new("U12345678".to_string());
        let identity = IdentityLink::create_with_slack(email.clone(), slack_id.clone());

        assert_eq!(identity.email(), &email);
        assert!(identity.is_slack_linked());
        assert_eq!(identity.slack_user_id(), Some(&slack_id));
    }

    #[test]
    fn test_link_slack() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let identity = IdentityLink::create_from_email(email);

        let slack_id = SlackUserId::new("U12345678".to_string());
        let result = identity.link_slack(slack_id.clone());

        assert!(result.is_ok());
        let linked_identity = result.unwrap();
        assert!(linked_identity.is_slack_linked());
        assert_eq!(linked_identity.slack_user_id(), Some(&slack_id));
    }

    #[test]
    fn test_link_slack_already_linked() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let slack_id = SlackUserId::new("U12345678".to_string());
        let identity = IdentityLink::create_with_slack(email, slack_id);

        let new_slack_id = SlackUserId::new("U87654321".to_string());
        let result = identity.link_slack(new_slack_id);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), IdentityLinkError::AlreadyLinked);
    }
}
