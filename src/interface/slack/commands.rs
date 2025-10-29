use crate::application::usecases::grant_user_resource_access::GrantUserResourceAccessUseCase;
use crate::domain::aggregates::identity_link::value_objects::SlackUserId;
use crate::domain::common::EmailAddress;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slackコマンドハンドラ
pub struct SlackCommandHandler {
    grant_access_usecase: Arc<GrantUserResourceAccessUseCase>,
}

impl SlackCommandHandler {
    pub fn new(grant_access_usecase: Arc<GrantUserResourceAccessUseCase>) -> Self {
        Self {
            grant_access_usecase,
        }
    }

    /// Slashコマンドをルーティング
    pub async fn route_slash_command(
        &self,
        event: SlackCommandEvent,
    ) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
        let command = event.command.0.as_str();
        let text = event.text.as_ref().map(|s| s.as_str()).unwrap_or("");
        let user_id = SlackUserId::new(event.user_id.to_string());
        let response_url = event.response_url.clone();

        match command {
            "/register-calendar" => {
                if text.is_empty() {
                    return Ok(SlackCommandEventResponse::new(
                        SlackMessageContent::new().with_text(
                            "使い方: `/register-calendar <your-email@gmail.com>`".to_string(),
                        ),
                    ));
                }

                // 即座に応答
                let immediate_response = SlackCommandEventResponse::new(
                    SlackMessageContent::new().with_text("⏳ 処理中...".to_string()),
                );

                // バックグラウンドで処理
                let grant_access_usecase = self.grant_access_usecase.clone();
                let email_str = text.to_string();

                tokio::spawn(async move {
                    let result = match EmailAddress::new(email_str.trim().to_string()) {
                        Ok(email) => grant_access_usecase
                            .execute(user_id.clone(), email.clone())
                            .await
                            .map(|_| {
                                format!(
                                    "✅ 登録完了！カレンダーへのアクセス権を付与しました: {}",
                                    email.as_str()
                                )
                            })
                            .map_err(|e| format!("❌ カレンダー登録に失敗: {}", e)),
                        Err(e) => Err(format!("❌ メールアドレスの形式が不正です: {}", e)),
                    };

                    // 結果をSlackに送信
                    let message = match result {
                        Ok(msg) => msg,
                        Err(err) => err,
                    };

                    println!("response_url経由でフォローアップ送信");
                    // response_urlを使ってメッセージを送信（Botがチャンネルに参加していなくてもOK）
                    let payload = serde_json::json!({
                        "text": message,
                        "response_type": "in_channel"
                    });

                    let client = reqwest::Client::new();
                    if let Err(e) = client
                        .post(response_url.0.as_str())
                        .json(&payload)
                        .send()
                        .await
                    {
                        eprintln!("フォローアップメッセージの送信に失敗: {}", e);
                    } else {
                        println!("✅ フォローアップメッセージを送信しました");
                    }
                });

                Ok(immediate_response)
            }
            "/link-user" => {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() != 2 {
                    return Ok(SlackCommandEventResponse::new(
                        SlackMessageContent::new().with_text(
                            "使い方: `/link-user <@slack_user> <email@gmail.com>`".to_string(),
                        ),
                    ));
                }

                // 即座に応答
                let immediate_response = SlackCommandEventResponse::new(
                    SlackMessageContent::new().with_text("⏳ 処理中...".to_string()),
                );

                // バックグラウンドで処理
                let grant_access_usecase = self.grant_access_usecase.clone();
                let target_user_str = parts[0]
                    .trim_matches(|c| c == '<' || c == '>' || c == '@')
                    .to_string();
                let email_str = parts[1].to_string();

                tokio::spawn(async move {
                    let target_user_id = SlackUserId::new(target_user_str.clone());
                    let result = match EmailAddress::new(email_str.trim().to_string()) {
                        Ok(email) => grant_access_usecase
                            .execute(target_user_id.clone(), email.clone())
                            .await
                            .map(|_| {
                                format!(
                                    "✅ 紐付け完了！<@{}> に {} のカレンダーアクセス権を付与しました。",
                                    target_user_str,
                                    email.as_str()
                                )
                            })
                            .map_err(|e| format!("❌ ユーザー紐付けに失敗: {}", e)),
                        Err(e) => Err(format!("❌ メールアドレスの形式が不正です: {}", e)),
                    };

                    // 結果をSlackに送信
                    let message = match result {
                        Ok(msg) => msg,
                        Err(err) => err,
                    };

                    println!("response_url経由でフォローアップ送信");
                    // response_urlを使ってメッセージを送信（Botがチャンネルに参加していなくてもOK）
                    let payload = serde_json::json!({
                        "text": message,
                        "response_type": "in_channel"
                    });

                    let client = reqwest::Client::new();
                    if let Err(e) = client
                        .post(response_url.0.as_str())
                        .json(&payload)
                        .send()
                        .await
                    {
                        eprintln!("フォローアップメッセージの送信に失敗: {}", e);
                    } else {
                        println!("✅ フォローアップメッセージを送信しました");
                    }
                });

                Ok(immediate_response)
            }
            _ => Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!("不明なコマンド: {}", command)),
            )),
        }
    }
}
