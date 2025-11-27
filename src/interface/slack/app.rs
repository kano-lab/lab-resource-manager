//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::usecases::create_resource_usage::CreateResourceUsageUseCase;
use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::infrastructure::config::ResourceConfig;
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio_util::task::TaskTracker;

/// 依存性注入を備えたSlackアプリケーション
///
/// Slackインタラクションに必要なすべての依存関係を保持します。
pub struct SlackApp<R: ResourceUsageRepository> {
    // UseCases
    pub grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    pub create_resource_usage_usecase: Arc<CreateResourceUsageUseCase<R>>,

    // リポジトリ
    pub identity_repo: Arc<dyn IdentityLinkRepository>,

    // 設定
    pub resource_config: Arc<ResourceConfig>,

    // Slackインフラストラクチャ
    pub slack_client: Arc<SlackHyperClient>,
    pub bot_token: SlackApiToken,

    // セッション状態（user_id -> channel_id のマッピング）
    pub user_channel_map: Arc<RwLock<HashMap<SlackUserId, SlackChannelId>>>,

    // ランタイム
    pub task_tracker: TaskTracker,
    pub http_client: reqwest::Client,
}

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackApp<R> {
    /// 新しいSlackAppを作成
    ///
    /// # 引数
    /// * `grant_access_usecase` - ユーザーアクセス権限付与UseCase
    /// * `create_resource_usage_usecase` - リソース使用予定作成UseCase
    /// * `identity_repo` - ID紐付けリポジトリ
    /// * `resource_config` - リソース設定
    /// * `slack_client` - Slackクライアント
    /// * `bot_token` - Bot Token
    pub fn new(
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        create_resource_usage_usecase: Arc<CreateResourceUsageUseCase<R>>,
        identity_repo: Arc<dyn IdentityLinkRepository>,
        resource_config: Arc<ResourceConfig>,
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
    ) -> Self {
        Self {
            grant_access_usecase,
            create_resource_usage_usecase,
            identity_repo,
            resource_config,
            slack_client,
            bot_token,
            user_channel_map: Arc::new(RwLock::new(HashMap::new())),
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// すべてのバックグラウンドタスクの完了を待機
    ///
    /// シャットダウン時に呼び出して、グレースフルな終了を保証します
    pub async fn shutdown(&self) {
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }
}
