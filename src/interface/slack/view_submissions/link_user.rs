//! ユーザーリンクモーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{ACTION_LINK_EMAIL_INPUT, ACTION_USER_SELECT};
use crate::interface::slack::utility::extract_form_data as form_data;
use crate::interface::slack::views::modals::result;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// ユーザーリンクモーダル送信を処理
///
/// 他のユーザーをメールアドレスに紐付け、カレンダーアクセス権を付与（管理者用）
pub async fn handle<R: ResourceUsageRepository + Send + Sync + 'static>(
    app: &SlackApp<R>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("ユーザーリンクを処理中...");

    // ユーザーリンク処理を実行
    let link_result = async {
        // ユーザーIDを抽出
        let target_user_id = form_data::get_user_select(view_submission, ACTION_USER_SELECT)
            .ok_or("ユーザーが選択されていません")?;

        // メールアドレスを抽出
        let email_value = form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
            .ok_or("メールアドレスが入力されていません")?;

        // メールアドレスのバリデーション
        let email = EmailAddress::new(email_value.trim().to_string())
            .map_err(|e| format!("メールアドレスの形式が不正です: {}", e))?;

        // ユーザーをリンク
        app.grant_access_usecase
            .execute(ExternalSystem::Slack, target_user_id.clone(), email.clone())
            .await
            .map_err(|e| format!("紐付けに失敗しました: {}", e))?;

        Ok::<(String, EmailAddress), String>((target_user_id, email))
    }
    .await;

    match link_result {
        Ok((target_user_id, email)) => {
            info!(
                "✅ ユーザーリンク成功: {} -> {}",
                target_user_id,
                email.as_str()
            );

            // 成功モーダルを表示
            let success_modal = result::create_success_modal(
                "リンク成功",
                format!(
                    "ユーザー <@{}> をメールアドレス {} に紐付けました",
                    target_user_id,
                    email.as_str()
                ),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: success_modal,
                },
            )))
        }
        Err(e) => {
            error!("❌ ユーザーリンクに失敗: {}", e);

            // エラーモーダルを表示
            let error_modal = result::create_error_modal("リンク失敗", e);

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_modal },
            )))
        }
    }
}
