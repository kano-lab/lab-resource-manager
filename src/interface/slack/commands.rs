use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use slack_morphism::prelude::*;
use std::sync::Arc;
use tokio_util::task::TaskTracker;
use tracing::{error, info};

/// Slackコマンドハンドラ
pub struct SlackCommandHandler {
    grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
    task_tracker: TaskTracker,
    http_client: reqwest::Client,
}

impl SlackCommandHandler {
    /// 新しいSlackコマンドハンドラを作成
    ///
    /// # Arguments
    /// * `grant_access_usecase` - アクセス権付与ユースケース
    pub fn new(grant_access_usecase: Arc<GrantUserResourceAccessUseCase>) -> Self {
        Self {
            grant_access_usecase,
            task_tracker: TaskTracker::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// バックグラウンドタスクの完了を待機
    ///
    /// シャットダウン時に呼び出して、全てのバックグラウンドタスクの完了を待つ
    pub async fn shutdown(&self) {
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }

    /// Slashコマンドをルーティング
    pub async fn route_slash_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let command = event.command.0.as_str();
        let text = event.text.as_deref().unwrap_or("");
        let slack_user_id = event.user_id.to_string();
        let response_url = event.response_url.clone();

        match command {
            "/register-calendar" => {
                self.handle_register_calendar(text, slack_user_id, response_url)
                    .await
            }
            "/link-user" => self.handle_link_user(text, response_url).await,
            _ => Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!("不明なコマンド: {}", command)),
            )),
        }
    }

    async fn handle_register_calendar(
        &self,
        text: &str,
        slack_user_id: String,
        response_url: SlackResponseUrl,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        if text.is_empty() {
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text("使い方: `/register-calendar <your-email@gmail.com>`".to_string()),
            ));
        }

        let grant_access_usecase = self.grant_access_usecase.clone();
        let email_str = text.to_string();

        self.execute_with_background_response(response_url, || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(ExternalSystem::Slack, slack_user_id, email.clone())
                .await
                .map_err(|e| format!("❌ カレンダー登録に失敗: {}", e))?;

            Ok(format!(
                "✅ 登録完了！カレンダーへのアクセス権を付与しました: {}",
                email.as_str()
            ))
        })
        .await
    }

    async fn handle_link_user(
        &self,
        text: &str,
        response_url: SlackResponseUrl,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() != 2 {
            return Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text("使い方: `/link-user <@slack_user> <email@gmail.com>`".to_string()),
            ));
        }

        let grant_access_usecase = self.grant_access_usecase.clone();

        // Slackメンション形式のバリデーションとパース
        let slack_mention = parts[0].trim();
        let target_slack_user_id = slack_mention
            .strip_prefix("<@")
            .and_then(|s| s.strip_suffix(">"))
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string());

        let target_slack_user_id = match target_slack_user_id {
            Some(id) => id,
            None => {
                return Ok(SlackCommandEventResponse::new(
                    SlackMessageContent::new()
                        .with_text("❌ Slackユーザーの形式が不正です。".to_string()),
                ));
            }
        };

        let email_str = parts[1].to_string();

        self.execute_with_background_response(response_url, || async move {
            let email = EmailAddress::new(email_str.trim().to_string())
                .map_err(|e| format!("❌ メールアドレスの形式が不正です: {}", e))?;

            grant_access_usecase
                .execute(
                    ExternalSystem::Slack,
                    target_slack_user_id.clone(),
                    email.clone(),
                )
                .await
                .map_err(|e| format!("❌ ユーザー紐付けに失敗: {}", e))?;

            Ok(format!(
                "✅ 紐付け完了！<@{}> に {} のカレンダーアクセス権を付与しました。",
                target_slack_user_id,
                email.as_str()
            ))
        })
        .await
    }

    /// バックグラウンドで処理を実行し、結果をSlackに送信する共通ヘルパー
    ///
    /// TaskTrackerを使用してタスクを追跡し、シャットダウン時のグレースフル終了を可能にする
    async fn execute_with_background_response<F, Fut>(
        &self,
        response_url: SlackResponseUrl,
        operation: F,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
    {
        let http_client = self.http_client.clone();
        self.task_tracker.spawn(async move {
            let message = match operation().await {
                Ok(msg) => msg,
                Err(err) => err,
            };

            Self::send_followup_message_static(&http_client, &response_url, message).await;
        });

        Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new().with_text("⏳ 処理中...".to_string()),
        ))
    }

    /// Slackにフォローアップメッセージを送信
    ///
    /// バックグラウンドタスクから呼び出すための静的メソッド
    async fn send_followup_message_static(
        http_client: &reqwest::Client,
        response_url: &SlackResponseUrl,
        message: String,
    ) {
        let payload = serde_json::json!({
            "text": message,
            "response_type": "in_channel"
        });

        match http_client
            .post(response_url.0.as_str())
            .json(&payload)
            .send()
            .await
        {
            Ok(_) => info!("✅ フォローアップメッセージを送信しました"),
            Err(e) => error!("フォローアップメッセージの送信に失敗: {}", e),
        }
    }
}
