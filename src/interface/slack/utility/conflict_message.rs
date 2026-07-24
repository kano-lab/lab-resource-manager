//! リソース競合メッセージ構築
//!
//! `ApplicationError::ResourceConflict`が持つ構造化データ（競合したリソースと
//! 既存の使用予定）から、通知メッセージと同じ仕組み（`TemplateRenderer`）を使って
//! ユーザー向けメッセージを組み立てます。
//!
//! 競合先リソースに設定された通知設定（テンプレート・フォーマットスタイル・
//! タイムゾーン）を流用することで、`/reserve`と更新モーダルの両方で
//! 一貫した表示になります。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::infrastructure::config::ResourceConfig;
use crate::infrastructure::notifier::template_renderer::TemplateRenderer;
use crate::interface::slack::utility::user_resolver;
use std::sync::Arc;

/// リソース競合エラーからユーザー向けメッセージを構築
///
/// # 引数
/// * `resource` - 競合したリソース
/// * `existing_usage` - 競合している既存の使用予定
/// * `resource_config` - リソース設定（競合先リソースの通知設定を参照するため）
/// * `identity_repo` - ID紐付けリポジトリ（既存予約の所有者表示名解決のため）
pub async fn build(
    resource: &Resource,
    existing_usage: &ResourceUsage,
    resource_config: &ResourceConfig,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> String {
    let owner_display =
        user_resolver::resolve_display_name(existing_usage.owner_email(), identity_repo).await;

    // 競合先リソースに紐づく通知設定を流用し、テンプレート/フォーマットを一致させる
    let notification_config = resource_config
        .get_notifications_for_resource(resource)
        .into_iter()
        .next();

    let customization = notification_config
        .as_ref()
        .map(|c| c.customization())
        .unwrap_or_default();
    let timezone_owned = notification_config
        .as_ref()
        .and_then(|c| c.timezone())
        .map(str::to_string);

    let renderer = TemplateRenderer::new(
        &customization.templates,
        &customization.format,
        timezone_owned.as_deref(),
    );

    renderer.render_conflict(resource, existing_usage, &owner_display)
}
