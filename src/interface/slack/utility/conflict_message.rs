//! リソース競合メッセージ構築
//!
//! `ApplicationError::ResourceConflict`が持つ構造化データ（競合したリソースと
//! 既存の使用予定の一覧）から、通知メッセージと同じ仕組み（`TemplateRenderer`）を
//! 使ってユーザー向けメッセージを組み立てます。
//!
//! 複数リソースをまとめて予約しようとして複数件が競合した場合は、
//! 1件ずつメッセージを組み立てて連結する。
//!
//! 競合先リソースに設定された通知設定（テンプレート・フォーマットスタイル・
//! タイムゾーン）を流用することで、`/reserve`と更新モーダルの両方で
//! 一貫した表示になります。

use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::services::resource_usage::errors::ResourceConflictError;
use crate::infrastructure::config::ResourceConfig;
use crate::infrastructure::notifier::template_renderer::TemplateRenderer;
use crate::interface::slack::utility::user_resolver;
use std::sync::Arc;

/// リソース競合エラー（複数件）からユーザー向けメッセージを構築
///
/// # 引数
/// * `conflicts` - 競合した全件（競合したリソースと既存の使用予定の組）
/// * `resource_config` - リソース設定（競合先リソースの通知設定を参照するため）
/// * `identity_repo` - ID紐付けリポジトリ（既存予約の所有者表示名解決のため）
pub async fn build(
    conflicts: &[ResourceConflictError],
    resource_config: &ResourceConfig,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> String {
    let mut messages = Vec::with_capacity(conflicts.len());
    for conflict in conflicts {
        messages.push(build_one(conflict, resource_config, identity_repo).await);
    }
    messages.join("\n\n")
}

/// 1件分のリソース競合エラーからユーザー向けメッセージを構築
async fn build_one(
    conflict: &ResourceConflictError,
    resource_config: &ResourceConfig,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> String {
    let owner_display =
        user_resolver::resolve_display_name(conflict.existing_usage.owner_email(), identity_repo)
            .await;

    // 競合先リソースに紐づく通知設定を流用し、テンプレート/フォーマットを一致させる。
    // まとめた競合は同一予約に属するため、どのリソースを見ても同じ設定になる。
    let notification_config = conflict
        .resources
        .first()
        .map(|resource| resource_config.get_notifications_for_resource(resource))
        .unwrap_or_default()
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

    renderer.render_conflict(
        &conflict.resources,
        &conflict.existing_usage,
        &owner_display,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::entity::IdentityLink;
    use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use crate::domain::common::EmailAddress;
    use crate::domain::ports::repositories::RepositoryError;
    use crate::infrastructure::config::{DeviceConfig, ServerConfig};
    use async_trait::async_trait;
    use chrono::{Duration, Utc};

    /// 表示名を解決しないテスト用リポジトリ（メールアドレスがそのまま表示される）
    struct NoLinks;

    #[async_trait]
    impl IdentityLinkRepository for NoLinks {
        async fn find_by_email(
            &self,
            _email: &EmailAddress,
        ) -> Result<Option<IdentityLink>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_external_user_id(
            &self,
            _system: &ExternalSystem,
            _user_id: &str,
        ) -> Result<Option<IdentityLink>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save(&self, _identity_link: IdentityLink) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn gpu(device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))
    }

    fn config() -> ResourceConfig {
        ResourceConfig {
            servers: vec![ServerConfig {
                name: "Thalys".to_string(),
                calendar_id: "cal".to_string(),
                devices: (0..4)
                    .map(|id| DeviceConfig {
                        id,
                        model: "A100 80GB PCIe".to_string(),
                    })
                    .collect(),
                notifications: vec![],
            }],
            rooms: vec![],
        }
    }

    fn reservation(owner: &str, resources: Vec<Resource>) -> ResourceUsage {
        let now = Utc::now();
        ResourceUsage::new(
            EmailAddress::new(owner.to_string()).unwrap(),
            TimePeriod::new(now, now + Duration::hours(2)).unwrap(),
            resources,
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn conflicts_with_one_reservation_are_reported_as_one_message() {
        let existing = reservation("owner@example.com", vec![gpu(0), gpu(1), gpu(2)]);
        let conflicts = vec![ResourceConflictError::new(
            vec![gpu(0), gpu(1), gpu(2)],
            existing,
        )];
        let repo: Arc<dyn IdentityLinkRepository> = Arc::new(NoLinks);

        let message = build(&conflicts, &config(), &repo).await;

        assert_eq!(
            message.matches("予約が重複しています").count(),
            1,
            "同じ予約との競合は1つのメッセージにまとめるべき: {message}"
        );
        for device in ["GPU:0", "GPU:1", "GPU:2"] {
            assert!(
                message.contains(device),
                "競合した全リソースを載せるべき ({device}): {message}"
            );
        }
    }

    #[tokio::test]
    async fn conflicts_with_separate_reservations_are_reported_separately() {
        let first = reservation("owner-a@example.com", vec![gpu(0)]);
        let second = reservation("owner-b@example.com", vec![gpu(1)]);
        let conflicts = vec![
            ResourceConflictError::new(vec![gpu(0)], first),
            ResourceConflictError::new(vec![gpu(1)], second),
        ];
        let repo: Arc<dyn IdentityLinkRepository> = Arc::new(NoLinks);

        let message = build(&conflicts, &config(), &repo).await;

        assert_eq!(
            message.matches("予約が重複しています").count(),
            2,
            "予約が別なら、予約者ごとに分けて伝えるべき: {message}"
        );
        assert!(message.contains("owner-a@example.com"));
        assert!(message.contains("owner-b@example.com"));
    }
}
