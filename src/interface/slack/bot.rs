use super::commands::SlackCommandHandler;
use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::infrastructure::config::ResourceConfig;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slack Socket Mode Bot
///
/// Socket Modeで使用される簡易ラッパー。
/// 実際のSocket Modeサーバーのセットアップはバイナリ（main.rsまたはbinファイル）で
/// slack-morphismのSocket Mode機能を使用して行う必要がある。
pub struct SlackBot<R: ResourceUsageRepository> {
    command_handler: Arc<SlackCommandHandler<R>>,
    client: Arc<SlackHyperClient>,
}

impl<R: ResourceUsageRepository + Send + Sync + 'static> SlackBot<R> {
    /// 新しいボットインスタンスを作成
    ///
    /// # 引数
    /// * `bot_token` - Bot User OAuth Token (xoxb-...)
    /// * `command_handler` - コマンドハンドラ
    pub async fn new(
        command_handler: Arc<SlackCommandHandler<R>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        Ok(Self {
            command_handler,
            client,
        })
    }

    /// 一時的なコンストラクタ（後で削除予定）
    pub async fn new_temp(
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        let command_handler = Arc::new(
            SlackCommandHandler::new(grant_access_usecase).with_slack_client(client.clone()),
        );

        Ok(Self {
            command_handler,
            client,
        })
    }

    /// リソース設定付きでボットインスタンスを作成
    pub async fn new_with_config(
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        config: Arc<ResourceConfig>,
        bot_token: SlackApiToken,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        let command_handler = Arc::new(
            SlackCommandHandler::new(grant_access_usecase)
                .with_resource_config(config)
                .with_slack_client(client.clone())
                .with_bot_token(bot_token),
        );

        Ok(Self {
            command_handler,
            client,
        })
    }

    /// リソース管理機能完全版
    pub async fn new_with_resource_management(
        grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
        usage_repository: Arc<R>,
        identity_repo: Arc<dyn crate::domain::ports::repositories::IdentityLinkRepository>,
        config: Arc<ResourceConfig>,
        bot_token: SlackApiToken,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        let command_handler = Arc::new(
            SlackCommandHandler::new(grant_access_usecase)
                .with_resource_usage(usage_repository, identity_repo)
                .with_resource_config(config)
                .with_slack_client(client.clone())
                .with_bot_token(bot_token),
        );

        Ok(Self {
            command_handler,
            client,
        })
    }

    /// Slashコマンドを処理
    pub async fn handle_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.command_handler.route_slash_command(event).await
    }

    /// インタラクション（ボタンクリックなど）を処理
    pub async fn handle_interaction(
        &self,
        event: SlackInteractionEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.command_handler.handle_interaction(event).await
    }

    /// モーダル送信を処理
    pub async fn handle_view_submission(
        &self,
        view: SlackView,
        user_id: SlackUserId,
    ) -> Result<SlackViewSubmissionResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.command_handler
            .handle_view_submission(view, user_id)
            .await
    }

    /// クライアントへの参照を取得
    pub fn client(&self) -> Arc<SlackHyperClient> {
        self.client.clone()
    }
}
