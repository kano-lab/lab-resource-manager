//! /register-calendar コマンドハンドラ（非推奨）

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::async_execution::background_task;
use slack_morphism::prelude::*;
use tracing::info;

/// /register-calendar スラッシュコマンドを処理（非推奨）
///
/// メールアドレスを登録し、バックグラウンドでカレンダーアクセス権を付与
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let text = event.text.as_deref().unwrap_or("");
    let user_id = event.user_id.to_string();
    let response_url = event.response_url;

    if text.is_empty() {
        return Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("⚠️  このコマンドは非推奨です。代わりに `/reserve` コマンドを使用してください。\n\n使い方: `/register-calendar <your-email@gmail.com>`".to_string()),
        ));
    }

    // Log deprecation warning
    info!(
        "⚠️  非推奨コマンド /register-calendar が使用されました: user={}",
        user_id
    );

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
                "✅ 登録完了！カレンダーへのアクセス権を付与しました: {}\n\n⚠️  今後は `/reserve` コマンドを使用してください。このコマンドは非推奨です。",
                email.as_str()
            ))
        },
    )
    .await)
}
