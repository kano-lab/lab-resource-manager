//! /mcp-token コマンドハンドラ

use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::user_resolver;
use slack_morphism::prelude::*;
use tracing::{debug, info, warn};

/// /mcp-token スラッシュコマンドを処理
///
/// 呼び出したSlackユーザーに紐付いたメールアドレス宛にMCPアクセストークンを発行し、
/// 本人にのみ見えるエフェメラルメッセージで返す。再実行すると新しいトークンが発行され、
/// 古いトークンは失効する。
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    debug!(user = %event.user_id, "issuing an mcp token");

    let email_str = match user_resolver::resolve_user_email(
        &event.user_id,
        &app.repositories().identity_link,
    )
    .await
    {
        Ok(email) => email,
        Err(e) => {
            warn!(user = %event.user_id, error = %e, "could not resolve the caller to an email address");
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!(
                    "❌ メールアドレスが紐付けられていません。管理者に `/link-user` での紐付けを依頼してください。（{}）",
                    e
                )),
            ));
        }
    };

    let email = EmailAddress::new(email_str)?;

    let token = app.repositories().mcp_token.issue_token(&email).await?;

    info!(owner = %email.as_str(), "mcp token issued");

    let message = format!(
        "🔑 MCPアクセストークンを発行しました。\n\
        再実行すると新しいトークンが発行され、このトークンは失効します。\n\
        このメッセージは他の人には見えません。他人に共有しないでください。\n\n\
        ```\n{}\n```",
        token
    );

    Ok(SlackCommandEventResponse::new(
        SlackMessageContent::new().with_text(message),
    ))
}
