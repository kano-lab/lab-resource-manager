//! /link-user コマンドハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::async_execution::background_task;
use crate::interface::slack::slack_client::modals;
use crate::interface::slack::views;
use slack_morphism::prelude::*;
use tracing::info;

/// /link-user スラッシュコマンドを処理
///
/// モーダルを開いて他のユーザーをメールアドレスに紐付けるか、引数で直接紐付ける（後方互換性）
/// 管理者コマンド
pub async fn handle(
    app: &SlackApp,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let text = event.text.as_deref().unwrap_or("");

    // 引数なし: モーダルを開く
    if text.is_empty() {
        info!("🔗 ユーザーリンクモーダルを開きます");

        // ユーザーリンクモーダルを作成
        let modal = views::modals::link_user::create();

        // モーダルを開く
        modals::open(&app.slack_client, &app.bot_token, &event.trigger_id, modal).await?;

        // 空のレスポンスを返す（モーダルが開かれたことをSlackに伝える）
        return Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new(),
        ));
    }

    // 引数あり: 後方互換性のため、直接紐付け処理を実行
    info!("🔗 ユーザーを直接紐付け");

    let response_url = event.response_url;
    let parts: Vec<&str> = text.split_whitespace().collect();

    if parts.len() != 2 {
        return Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("使い方: `/link-user <@slack_user> <email@gmail.com>`".to_string()),
        ));
    }

    let grant_access_usecase = app.grant_access_usecase.clone();

    // Validate and parse Slack mention format
    let slack_mention = parts[0].trim();
    let target_slack_user_id = slack_mention
        .strip_prefix("<@")
        .and_then(|s| s.strip_suffix(">"))
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string());

    let target_slack_user_id = match target_slack_user_id {
        Some(id) => id,
        None => {
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text("❌ Slackユーザーの形式が不正です。".to_string()),
            ));
        }
    };

    let email_str = parts[1].to_string();

    // Execute in background
    Ok(background_task::execute_with_response(
        &app.task_tracker,
        app.http_client.clone(),
        response_url,
        || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(
                    ExternalSystem::Slack,
                    target_slack_user_id.clone(),
                    email.clone(),
                )
                .await
                .map_err(|e| format!("❌ ユーザー紐付けに失敗: {}", e))?;

            Ok(format!(
                "✅ 紐付け完了！<@{}> に {} のカレンダーアクセス権を付与しました。",
                target_slack_user_id,
                email.as_str()
            ))
        },
    )
    .await)
}
