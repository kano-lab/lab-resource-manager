//! ユーザーリンクモーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{ACTION_LINK_EMAIL_INPUT, ACTION_USER_SELECT};
use crate::interface::slack::utility::extract_form_data;
use slack_morphism::prelude::*;
use tracing::{error, info};

/// ユーザーリンクモーダル送信を処理
///
/// 他のユーザーをメールアドレスに紐付け、カレンダーアクセス権を付与（管理者用）
pub async fn handle(
    app: &SlackApp,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>> {
    info!("ユーザーリンクを処理中...");

    // リンク処理を実行
    let link_result = async {
        // ユーザーIDを抽出
        let target_user_id =
            extract_form_data::get_user_select(view_submission, ACTION_USER_SELECT)
                .ok_or("ユーザーが選択されていません")?;

        // メールアドレスを抽出
        let email_value =
            extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
                .ok_or("メールアドレスが入力されていません")?;

        // メールアドレスのバリデーション
        let email = EmailAddress::new(email_value.trim().to_string())
            .map_err(|e| format!("メールアドレスの形式が不正です: {}", e))?;

        // ユーザーをリンク
        app.grant_access_usecase
            .execute(
                ExternalSystem::Slack,
                target_user_id.clone(),
                email.clone(),
            )
            .await
            .map_err(|e| format!("ユーザーリンクに失敗しました: {}", e))?;

        Ok::<(String, EmailAddress), String>((target_user_id, email))
    }
    .await;

    match link_result {
        Ok((user_id, email)) => {
            info!("✅ ユーザーリンク成功: {} -> {}", user_id, email.as_str());

            // 成功時はモーダルを閉じる（モーダルが閉じることで成功を示す）
            Ok(None)
        }
        Err(e) => {
            error!("❌ ユーザーリンクに失敗: {}", e);

            // エラー時はモーダルに表示
            Ok(Some(SlackViewSubmissionResponse::Errors(
                SlackViewSubmissionErrorsResponse {
                    errors: [(
                        ACTION_LINK_EMAIL_INPUT.to_string(),
                        format!("紐付けに失敗しました: {}", e),
                    )]
                    .into_iter()
                    .collect(),
                },
            )))
        }
    }
}
