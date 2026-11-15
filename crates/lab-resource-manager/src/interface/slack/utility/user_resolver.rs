//! ユーザーID解決
//!
//! SlackユーザーIDとメールアドレスの相互解決を行います

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::IdentityLinkRepository;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// SlackユーザーIDをメールアドレスに解決
///
/// # 引数
/// * `slack_user_id` - SlackユーザーID
/// * `identity_repo` - ID紐付けリポジトリ
///
/// # 戻り値
/// 見つかった場合はメールアドレス
pub async fn resolve_user_email(
    slack_user_id: &SlackUserId,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let identity_link = identity_repo
        .find_by_external_user_id(&ExternalSystem::Slack, slack_user_id.as_ref())
        .await?
        .ok_or_else(|| {
            format!(
                "Slackユーザー {} に対応するメールアドレスが見つかりません",
                slack_user_id
            )
        })?;

    Ok(identity_link.email().as_str().to_string())
}

/// ユーザーがメールアドレスに紐付けされているかチェック
///
/// # 引数
/// * `slack_user_id` - SlackユーザーID
/// * `identity_repo` - ID紐付けリポジトリ
///
/// # 戻り値
/// 紐付けされている場合はtrue、そうでない場合はfalse
pub async fn is_user_linked(
    slack_user_id: &SlackUserId,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> bool {
    identity_repo
        .find_by_external_user_id(&ExternalSystem::Slack, slack_user_id.as_ref())
        .await
        .ok()
        .flatten()
        .is_some()
}

/// メールアドレスをSlack表示用文字列に解決
///
/// 紐付けが見つかりSlackのIDが登録されていればメンション形式（`<@U12345>`）、
/// それ以外はメールアドレスそのものを返す。
///
/// # 引数
/// * `email` - 解決対象のメールアドレス
/// * `identity_repo` - ID紐付けリポジトリ
pub async fn resolve_display_name(
    email: &EmailAddress,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> String {
    let identity_link = identity_repo.find_by_email(email).await.ok().flatten();

    identity_link
        .as_ref()
        .and_then(|link| link.get_identity_for_system(&ExternalSystem::Slack))
        .map(|slack_identity| format!("<@{}>", slack_identity.user_id()))
        .unwrap_or_else(|| email.as_str().to_string())
}
