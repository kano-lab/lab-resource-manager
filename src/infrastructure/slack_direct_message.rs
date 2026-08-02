//! 本人宛のSlack DM送信
//!
//! 予約にまつわる知らせのうち、チャンネルへのブロードキャストではなく本人に直接
//! 届けるもの（事後予約の提案、無断使用の通知、未使用予約のお知らせ）はここを通る。
//! 誰に届けるかはメールアドレスで指定し、Slackのユーザーへの読み替えはここで閉じる。

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::NotificationError;
use crate::domain::ports::repositories::IdentityLinkRepository;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// 送信できたメッセージの位置
///
/// 受け取った側から「この知らせはおかしい」と言われたとき、チャンネルと時刻の組で
/// 当該メッセージを特定できる。何の知らせだったかは送り手にしか分からないため、
/// ログに残すのは呼び出し側の仕事とする。
pub struct SentDirectMessage {
    pub channel: SlackChannelId,
    pub ts: SlackTs,
}

/// メールアドレス宛にSlack DMを送る
pub struct SlackDirectMessenger {
    slack_client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,
    identity_repo: Arc<dyn IdentityLinkRepository>,
}

impl SlackDirectMessenger {
    /// 新しい送信口を作成
    pub fn new(
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        Self {
            slack_client,
            bot_token,
            identity_repo,
        }
    }

    /// 本文とブロックを届ける
    ///
    /// `blocks`が空ならテキストだけのメッセージとして送る。
    ///
    /// # Errors
    /// 宛先のSlackアカウントが分からない場合、またはSlack APIの呼び出しに失敗した場合
    pub async fn send(
        &self,
        to: &EmailAddress,
        text: String,
        blocks: Vec<SlackBlock>,
    ) -> Result<SentDirectMessage, NotificationError> {
        let slack_user_id = self.resolve_slack_user_id(to).await?;
        let session = self.slack_client.open_session(&self.bot_token);

        let opened = session
            .conversations_open(
                &SlackApiConversationsOpenRequest::new().with_users(vec![slack_user_id]),
            )
            .await
            .map_err(|e| {
                NotificationError::SendFailure(format!("DMチャンネルのオープンに失敗: {}", e))
            })?;

        let content = SlackMessageContent::new().with_text(text);
        let content = if blocks.is_empty() {
            content
        } else {
            content.with_blocks(blocks)
        };

        let sent = session
            .chat_post_message(&SlackApiChatPostMessageRequest::new(
                opened.channel.id,
                content,
            ))
            .await
            .map_err(|e| NotificationError::SendFailure(format!("Slack DM送信失敗: {}", e)))?;

        Ok(SentDirectMessage {
            channel: sent.channel,
            ts: sent.ts,
        })
    }

    /// 宛先のSlackユーザーIDを解決する
    async fn resolve_slack_user_id(
        &self,
        to: &EmailAddress,
    ) -> Result<SlackUserId, NotificationError> {
        let identity_link = self
            .identity_repo
            .find_by_email(to)
            .await
            .map_err(|e| {
                NotificationError::RepositoryError(format!("IdentityLink取得失敗: {}", e))
            })?
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "IdentityLink未登録のためDMを送信できません: {}",
                    to.as_str()
                ))
            })?;

        let slack_identity = identity_link
            .get_identity_for_system(&ExternalSystem::Slack)
            .ok_or_else(|| {
                NotificationError::SendFailure(format!(
                    "Slackアカウント未リンクのためDMを送信できません: {}",
                    to.as_str()
                ))
            })?;

        Ok(SlackUserId::new(slack_identity.user_id().to_string()))
    }
}
