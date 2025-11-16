//! メールアドレス登録モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::ACTION_EMAIL_INPUT;
use crate::interface::slack::utility::extract_form_data;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// メールアドレス登録モーダル送信を処理
///
/// メールアドレスを登録し、カレンダーアクセス権を付与
pub async fn handle(
    app: &SlackApp,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("メールアドレス登録を処理中...");

    let user_id = view_submission.user.id.clone();

    // Extract email from form
    let email_value = extract_form_data::get_plain_text_input(view_submission, ACTION_EMAIL_INPUT)
        .ok_or("メールアドレスが入力されていません")?;

    // Validate email
    let email_result = EmailAddress::new(email_value.trim().to_string());

    // Register user
    let registration_result = match &email_result {
        Ok(email) => app
            .grant_access_usecase
            .execute(ExternalSystem::Slack, user_id.to_string(), email.clone())
            .await
            .map_err(|e| e.into()),
        Err(e) => Err(Box::new(e.clone()) as Box<dyn std::error::Error + Send + Sync>),
    };

    // channel_id を取得
    let channel_id = app.user_channel_map.read().unwrap().get(&user_id).cloned();

    if let Some(channel_id) = channel_id {
        // エフェメラルメッセージで結果を送信
        let message_text = match registration_result {
            Ok(_) => {
                info!(
                    "✅ ユーザー登録成功: {}",
                    email_result.as_ref().unwrap().as_str()
                );
                format!(
                    "✅ メールアドレス {} を登録しました",
                    email_result.as_ref().unwrap().as_str()
                )
            }
            Err(e) => {
                error!("❌ ユーザー登録に失敗: {}", e);
                format!("❌ 登録に失敗しました: {}", e)
            }
        };

        let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
            channel_id,
            user_id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );

        let session = app.slack_client.open_session(&app.bot_token);
        session.chat_post_ephemeral(&ephemeral_req).await?;
    } else {
        error!("❌ channel_id が見つかりません");
    }

    // モーダルを閉じる
    Ok(None)
}
