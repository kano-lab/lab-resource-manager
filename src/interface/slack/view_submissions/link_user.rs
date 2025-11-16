//! ユーザーリンクモーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{ACTION_LINK_EMAIL_INPUT, ACTION_USER_SELECT};
use crate::interface::slack::utility::extractors::form_data;
use crate::interface::slack::views::messages;
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
        let target_user_id = form_data::get_user_select(view_submission, ACTION_USER_SELECT)
            .ok_or("ユーザーが選択されていません")?;

        // メールアドレスを抽出
        let email_value =
            form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
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

            // 成功メッセージをモーダルに表示
            let success_message = format!(
                "紐付け完了\n\n<@{}> に {} のカレンダーアクセス権を付与しました。",
                user_id,
                email.as_str()
            );
            let confirmation_modal =
                messages::confirmation::create_confirmation_modal("紐付け完了", &success_message);

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse {
                    view: confirmation_modal,
                },
            )))
        }
        Err(e) => {
            error!("❌ ユーザーリンクに失敗: {}", e);

            // エラーメッセージをモーダルに表示
            let error_modal = messages::error::create_error_modal(
                "紐付け失敗",
                format!("ユーザーの紐付けに失敗しました\n\n{}", e),
            );

            Ok(Some(SlackViewSubmissionResponse::Update(
                SlackViewSubmissionUpdateResponse { view: error_modal },
            )))
        }
    }
}
