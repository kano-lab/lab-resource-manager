//! /register-calendar コマンドハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::async_execution::background_task;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /register-calendar スラッシュコマンドを処理
///
/// モーダルを開いてメールアドレスを登録するか、引数で直接登録する（後方互換性）
pub async fn handle(
    app: &SlackApp,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let text = event.text.as_deref().unwrap_or("");
    let user_id = event.user_id.to_string();

    // 引数なし: モーダルを開く
    if text.is_empty() {
        info!("📧 メールアドレス登録モーダルを開きます: user={}", user_id);

        // メールアドレス登録モーダルを作成
        let modal = views::modals::registration::create();

        // モーダルを開く
        modals::open(&app.slack_client, &app.bot_token, &event.trigger_id, modal).await?;

        // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
        return Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new(),
        ));
    }

    // 引数あり: 後方互換性のため、直接登録処理を実行
    info!("📧 メールアドレスを直接登録: user={}", user_id);

    let response_url = event.response_url;
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
