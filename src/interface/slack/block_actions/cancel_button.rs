//! 予約キャンセルボタンハンドラ

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::user_resolver;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// 予約キャンセルボタンのクリックを処理
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔵 cancel_button::handle が呼ばれました");

    let Some(usage_id_str) = &action.value else {
        error!("❌ usage_idが取得できませんでした");
        println!("❌ action.value is None");
        return Ok(());
    };

    println!("🔵 action.value = {}", usage_id_str);

    let Some(user) = &block_actions.user else {
        error!("❌ ユーザー情報が取得できませんでした");
        println!("❌ block_actions.user is None");
        return Ok(());
    };

    info!("🗑️ 予約キャンセル要求: usage_id={}", usage_id_str);
    println!("🗑️ 予約キャンセル要求: usage_id={}", usage_id_str);

    // 依存性を取得
    let delete_usage_usecase = &app.delete_usage_usecase;
    let identity_repo = &app.identity_repo;

    // ユーザーのメールアドレスを取得
    println!("🔵 ユーザーメールアドレス取得中...");
    let owner_email = user_resolver::resolve_user_email(&user.id, identity_repo).await?;
    println!("🔵 owner_email = {}", owner_email.as_str());

    // 予約を削除
    let usage_id = UsageId::from_string(usage_id_str.to_string());
    info!(
        "📍 削除処理開始: usage_id={}, owner={}",
        usage_id.as_str(),
        owner_email.as_str()
    );
    println!(
        "🔵 削除処理開始: usage_id={}, owner={}",
        usage_id.as_str(),
        owner_email.as_str()
    );

    let result = delete_usage_usecase
        .execute(&usage_id, &EmailAddress::new(owner_email.clone())?)
        .await;

    // ユーザーにフィードバックメッセージを送信
    if let Some(channel) = &block_actions.channel {
        let message_text = match &result {
            Ok(_) => {
                info!("✅ 削除成功: {}", usage_id.as_str());
                format!("✅ 予約をキャンセルしました")
            }
            Err(e) => {
                error!("❌ 削除失敗: usage_id={}, error={}", usage_id.as_str(), e);

                // エラーの種類に応じてユーザーフレンドリーなメッセージを返す
                let error_msg = e.to_string();
                if error_msg.contains("見つかりません") || error_msg.contains("NotFound") {
                    "❌ 申し訳ございません。この予約は既に削除されているか、見つかりませんでした。".to_string()
                } else if error_msg.contains("権限") || error_msg.contains("Unauthorized") {
                    "❌ この予約を削除する権限がありません。".to_string()
                } else {
                    format!("❌ 予約の削除に失敗しました: {}", error_msg)
                }
            }
        };

        // エフェメラルメッセージで結果を通知
        let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
            channel.id.clone(),
            user.id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );

        let session = app.slack_client.open_session(&app.bot_token);
        if let Err(e) = session.chat_post_ephemeral(&ephemeral_req).await {
            error!("❌ エフェメラルメッセージ送信失敗: {}", e);
        }
    }

    // エラーの場合もOkを返す（ユーザーには既にメッセージを送信済み）
    // これにより、Slackに「エラーが発生しました」というデフォルトメッセージが表示されない
    Ok(())
}
