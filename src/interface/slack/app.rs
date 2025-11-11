//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::usecases::{
    create_resource_usage::CreateResourceUsageUseCase,
    delete_resource_usage::DeleteResourceUsageUseCase,
    grant_user_resource_access::GrantUserResourceAccessUseCase,
    update_resource_usage::UpdateResourceUsageUseCase,
};
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::infrastructure::config::ResourceConfig;
use slack_morphism::prelude::*;
use std::sync::Arc;
use tokio_util::task::TaskTracker;

/// 依存性注入を備えたSlackアプリケーション
///
/// Slackインタラクションに必要なすべての依存関係を保持し、
/// builderパターンで設定を提供します。
pub struct SlackApp<R: ResourceUsageRepository> {
    // UseCases
    pub grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    pub create_usage_usecase: Option<Arc<CreateResourceUsageUseCase<R>>>,
    pub delete_usage_usecase: Option<Arc<DeleteResourceUsageUseCase<R>>>,
    pub update_usage_usecase: Option<Arc<UpdateResourceUsageUseCase<R>>>,

    // リポジトリ
    pub identity_repo: Option<Arc<dyn IdentityLinkRepository>>,

    // 設定
    pub resource_config: Option<Arc<ResourceConfig>>,

    // Slackインフラストラクチャ
    pub slack_client: Option<Arc<SlackHyperClient>>,
    pub bot_token: Option<SlackApiToken>,

    // ランタイム
    pub task_tracker: TaskTracker,
    pub http_client: reqwest::Client,
}

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackApp<R> {
    /// 最小限の依存関係で新しいSlackAppを作成
    ///
    /// # 引数
    /// * `grant_access_usecase` - ユーザーアクセス権限付与UseCase
    pub fn new(grant_access_usecase: Arc<GrantUserResourceAccessUseCase>) -> Self {
        Self {
            grant_access_usecase,
            create_usage_usecase: None,
            delete_usage_usecase: None,
            update_usage_usecase: None,
            identity_repo: None,
            resource_config: None,
            slack_client: None,
            bot_token: None,
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// リソース使用機能を追加（builderパターン）
    ///
    /// # 引数
    /// * `repository` - リソース使用リポジトリ
    /// * `identity_repo` - ID紐付けリポジトリ
    pub fn with_resource_usage(
        mut self,
        repository: Arc<R>,
        identity_repo: Arc<dyn IdentityLinkRepository>,
    ) -> Self {
        self.create_usage_usecase = Some(Arc::new(CreateResourceUsageUseCase::new(
            repository.clone(),
        )));
        self.delete_usage_usecase = Some(Arc::new(DeleteResourceUsageUseCase::new(
            repository.clone(),
        )));
        self.update_usage_usecase = Some(Arc::new(UpdateResourceUsageUseCase::new(repository)));
        self.identity_repo = Some(identity_repo);
        self
    }

    /// リソース設定を追加（builderパターン）
    pub fn with_resource_config(mut self, config: Arc<ResourceConfig>) -> Self {
        self.resource_config = Some(config);
        self
    }

    /// Slackクライアントを追加（builderパターン）
    pub fn with_slack_client(mut self, client: Arc<SlackHyperClient>) -> Self {
        self.slack_client = Some(client);
        self
    }

    /// Botトークンを追加（builderパターン）
    pub fn with_bot_token(mut self, token: SlackApiToken) -> Self {
        self.bot_token = Some(token);
        self
    }

    /// すべてのバックグラウンドタスクの完了を待機
    ///
    /// シャットダウン時に呼び出して、グレースフルな終了を保証します
    pub async fn shutdown(&self) {
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }
}
