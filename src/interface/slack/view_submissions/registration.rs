//! メールアドレス登録モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::ACTION_EMAIL_INPUT;
use crate::interface::slack::utility::extract_form_data;
use crate::interface::slack::views;
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
        let email_value =
            extract_form_data::get_plain_text_input(view_submission, ACTION_EMAIL_INPUT)
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

            // 成功モーダルに遷移
            let success_view = views::modals::result::create_success(
                "登録完了",
                &format!("メールアドレス {} を登録しました", email.as_str()),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: success_view },
            )))
        }
        Err(e) => {
            error!("❌ ユーザー登録に失敗: {}", e);

            // エラーモーダルに遷移
            let error_view = views::modals::result::create_error("登録失敗", &e);

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_view },
            )))
        }
    }
}
