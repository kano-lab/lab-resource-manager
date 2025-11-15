//! /register-calendar コマンドハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::async_execution::background_task;
use slack_morphism::prelude::*;

/// /register-calendar スラッシュコマンドを処理
///
/// メールアドレスを登録し、バックグラウンドでカレンダーアクセス権を付与
pub async fn handle(
    app: &SlackApp,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let text = event.text.as_deref().unwrap_or("");
    let user_id = event.user_id.to_string();
    let response_url = event.response_url;

    if text.is_empty() {
        return Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("使い方: `/register-calendar <your-email@gmail.com>`".to_string()),
        ));
    }

    let grant_access_usecase = app.grant_access_usecase.clone();
    let email_str = text.to_string();

    // Execute in background
    Ok(background_task::execute_with_response(
        &app.task_tracker,
        app.http_client.clone(),
        response_url,
        || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(ExternalSystem::Slack, user_id, email.clone())
                .await
                .map_err(|e| format!("❌ カレンダー登録に失敗: {}", e))?;

            Ok(format!(
                "✅ 登録完了！カレンダーへのアクセス権を付与しました: {}",
                email.as_str()
            ))
        },
    )
    .await)
}
