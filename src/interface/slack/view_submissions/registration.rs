//! メールアドレス登録モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::ACTION_EMAIL_INPUT;
use crate::interface::slack::extractors::form_data;
use crate::interface::slack::views::messages;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// メールアドレス登録モーダル送信を処理
///
/// メールアドレスを登録し、カレンダーアクセス権を付与
pub async fn handle(
    app: &SlackApp,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("メールアドレス登録を処理中...");

    // 登録処理を実行
    let registration_result = async {
        let user_id = view_submission.user.id.to_string();

        // Extract email from form
        let email_value = form_data::get_plain_text_input(view_submission, ACTION_EMAIL_INPUT)
            .ok_or("メールアドレスが入力されていません")?;

        // Validate email
        let email = EmailAddress::new(email_value.trim().to_string())
            .map_err(|e| format!("メールアドレスの形式が不正です: {}", e))?;

        // Register user
        app.grant_access_usecase
            .execute(ExternalSystem::Slack, user_id.clone(), email.clone())
            .await
            .map_err(|e| format!("登録に失敗しました: {}", e))?;

        Ok::<EmailAddress, String>(email)
    }
    .await;

    match registration_result {
        Ok(email) => {
            info!("✅ ユーザー登録成功: {}", email.as_str());

            // 成功メッセージをモーダルに表示
            let success_message = format!(
                "登録完了\n\nカレンダーへのアクセス権を付与しました: {}",
                email.as_str()
            );
            let confirmation_modal = messages::confirmation::create_confirmation_modal(
                "登録完了",
                &success_message,
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: confirmation_modal,
                },
            )))
        }
        Err(e) => {
            error!("❌ ユーザー登録に失敗: {}", e);

            // エラーメッセージをモーダルに表示
            let error_modal =
                messages::error::create_error_modal("登録失敗", format!("登録に失敗しました\n\n{}", e));

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_modal },
            )))
        }
    }
}
