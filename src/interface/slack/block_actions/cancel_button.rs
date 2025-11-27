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

    match &result {
        Ok(_) => println!("🔵 削除処理結果: OK"),
        Err(e) => println!("🔵 削除処理結果: Error = {}", e),
    }

    match &result {
        Ok(_) => {
            info!("✅ 削除成功: {}", usage_id.as_str());
        }
        Err(e) => {
            error!("❌ 削除失敗: usage_id={}, error={}", usage_id.as_str(), e);
        }
    }

    result?;

    info!("✅ 予約をキャンセルしました: {}", usage_id_str);
    Ok(())
}
