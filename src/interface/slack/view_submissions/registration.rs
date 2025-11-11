//! メールアドレス登録モーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::ACTION_EMAIL_INPUT;
use crate::interface::slack::extractors::form_data;
use crate::interface::slack::views::modals::{reservation, result};
use slack_morphism::prelude::*;
use tracing::{error, info};

/// メールアドレス登録モーダル送信を処理
///
/// メールアドレスを登録し、自動的に予約モーダルを開く
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
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

            // 成功時は予約モーダルをpush
            if let Some(config) = &app.resource_config {
                let initial_server = config.servers.first().map(|s| s.name.as_str());
                let reserve_modal =
                    reservation::create_reserve_modal(config, None, initial_server, None);

                info!("📋 予約モーダルをpushします...");
                Ok(Some(SlackViewSubmissionResponse::Push(
                    SlackViewSubmissionPushResponse {
                        view: reserve_modal,
                    },
                )))
            } else {
                // 設定がない場合は成功メッセージのみ
                let success_modal = result::create_success_modal(
                    "登録完了",
                    format!("メールアドレスを登録しました\n\n{}", email.as_str()),
                );

                Ok(Some(SlackViewSubmissionResponse::Update(
                    SlackViewSubmissionUpdateResponse {
                        view: success_modal,
                    },
                )))
            }
        }
        Err(e) => {
            error!("❌ ユーザー登録に失敗: {}", e);

            // エラーモーダルを表示
            let error_modal =
                result::create_error_modal("登録失敗", format!("登録に失敗しました\n\n{}", e));

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: error_modal,
                },
            )))
        }
    }
}
