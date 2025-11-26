//! ユーザーリンクモーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{ACTION_LINK_EMAIL_INPUT, ACTION_USER_SELECT};
use crate::interface::slack::utility::extract_form_data;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// ユーザーリンクモーダル送信を処理
///
/// 他のユーザーをメールアドレスに紐付け、カレンダーアクセス権を付与（管理者用）
pub async fn handle<R: ResourceUsageRepository>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("ユーザーリンクを処理中...");

    let user_id = view_submission.user.id.clone();

    // ユーザーIDを抽出
    let target_user_id = extract_form_data::get_user_select(view_submission, ACTION_USER_SELECT)
        .ok_or("ユーザーが選択されていません")?;

    // メールアドレスを抽出
    let email_value =
        extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
            .ok_or("メールアドレスが入力されていません")?;

    // メールアドレスのバリデーション
    let email_result = EmailAddress::new(email_value.trim().to_string());

    // ユーザーをリンク
    let link_result = match &email_result {
        Ok(email) => app
            .grant_access_usecase
            .execute(ExternalSystem::Slack, target_user_id.clone(), email.clone())
            .await
            .map_err(|e| e.into()),
        Err(e) => Err(Box::new(e.clone()) as Box<dyn std::error::Error + Send + Sync>),
    };

    // channel_id を取得
    let channel_id = app.user_channel_map.read().unwrap().get(&user_id).cloned();

    if let Some(channel_id) = channel_id {
        // エフェメラルメッセージで結果を送信
        let message_text = match link_result {
            Ok(_) => {
                info!(
                    "✅ ユーザーリンク成功: {} -> {}",
                    target_user_id,
                    email_result.as_ref().unwrap().as_str()
                );
                format!(
                    "✅ ユーザー <@{}> をメールアドレス {} に紐付けました",
                    target_user_id,
                    email_result.as_ref().unwrap().as_str()
                )
            }
            Err(e) => {
                error!("❌ ユーザーリンクに失敗: {}", e);
                format!("❌ 紐付けに失敗しました: {}", e)
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
