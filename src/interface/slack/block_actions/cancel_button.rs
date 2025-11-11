//! 予約キャンセルボタンハンドラ

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::adapters::user_resolver;
use crate::interface::slack::app::SlackApp;
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

    // Check dependencies
    let delete_usage_usecase = app
        .delete_usage_usecase
        .as_ref()
        .ok_or("DeleteUsageUseCaseが設定されていません")?;

    let identity_repo = app
        .identity_repo
        .as_ref()
        .ok_or("IdentityRepositoryが設定されていません")?;

    // Get user email
    let owner_email = user_resolver::resolve_user_email(&user.id, identity_repo).await?;

    // Delete reservation
    let usage_id = UsageId::new(usage_id_str.to_string());
    delete_usage_usecase
        .execute(
            &usage_id,
            &crate::domain::common::EmailAddress::new(owner_email)?,
        )
        .await?;

    info!("✅ 予約をキャンセルしました: {}", usage_id_str);
    Ok(())
}
