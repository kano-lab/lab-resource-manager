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
    let Some(usage_id_str) = &action.value else {
        error!("❌ usage_idが取得できませんでした");
        return Ok(());
    };

    let Some(user) = &block_actions.user else {
        error!("❌ ユーザー情報が取得できませんでした");
        return Ok(());
    };

    info!("🗑️ 予約キャンセル要求: usage_id={}", usage_id_str);

    // 依存性を取得
    let delete_usage_usecase = &app.delete_usage_usecase;
    let identity_repo = &app.identity_repo;

    // ユーザーのメールアドレスを取得
    let owner_email = user_resolver::resolve_user_email(&user.id, identity_repo).await?;

    // 予約を削除
    let usage_id = UsageId::from_string(usage_id_str.to_string());
    delete_usage_usecase
        .execute(&usage_id, &EmailAddress::new(owner_email)?)
        .await?;

    info!("✅ 予約をキャンセルしました: {}", usage_id_str);
    Ok(())
}
