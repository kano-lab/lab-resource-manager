//! Slackアプリケーションコア
//!
//! 依存関係を管理し、Slackインタラクションのメインエントリポイントを提供

use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::domain::ports::repositories::IdentityLinkRepository;
use slack_morphism::prelude::*;
use std::sync::Arc;
use tokio_util::task::TaskTracker;

/// 依存性注入を備えたSlackアプリケーション
///
/// Slackインタラクションに必要なすべての依存関係を保持します。
pub struct SlackApp {
    // UseCases
    pub grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,

    // リポジトリ
    pub identity_repo: Arc<dyn IdentityLinkRepository>,

    // Slackインフラストラクチャ
    pub slack_client: Arc<SlackHyperClient>,
    pub bot_token: SlackApiToken,

    // ランタイム
    pub task_tracker: TaskTracker,
    pub http_client: reqwest::Client,
}

impl SlackApp {
    /// 新しいSlackAppを作成
    ///
    /// # 引数
    /// * `grant_access_usecase` - ユーザーアクセス権限付与UseCase
    /// * `identity_repo` - ID紐付けリポジトリ
    /// * `slack_client` - Slackクライアント
    /// * `bot_token` - Bot Token
    pub fn new(
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        identity_repo: Arc<dyn IdentityLinkRepository>,
        slack_client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
    ) -> Self {
        Self {
            grant_access_usecase,
            identity_repo,
            slack_client,
            bot_token,
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
