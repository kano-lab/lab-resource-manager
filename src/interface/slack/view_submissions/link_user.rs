//! ユーザーリンクモーダル送信ハンドラ

use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::constants::{
    ACTION_LINK_EMAIL_INPUT, ACTION_LINK_OS_USERNAME_INPUT, ACTION_LINK_SERVER_SELECT,
    ACTION_LINK_TARGET_TYPE, ACTION_USER_SELECT,
};
use crate::interface::slack::utility::extract_form_data;
use slack_morphism::prelude::*;
use tracing::{debug, error, info};

/// リンク対象（外部システム上のユーザー識別情報）
struct LinkTarget {
    external_system: ExternalSystem,
    external_user_id: String,
    /// 結果メッセージ表示用のラベル（Slackなら`<@id>`、OSなら`username@server`）
    display: String,
}

/// フォームからリンク対象を抽出する
///
/// `target_type`が"os"の場合はサーバー選択とOSユーザー名入力から、
/// それ以外（デフォルト="slack"）の場合はSlackユーザー選択から抽出する。
fn extract_link_target(
    view_submission: &SlackInteractionViewSubmissionEvent,
    target_type: Option<&str>,
) -> Result<LinkTarget, Box<dyn std::error::Error + Send + Sync>> {
    if target_type == Some("os") {
        let server =
            extract_form_data::get_selected_option_text(view_submission, ACTION_LINK_SERVER_SELECT)
                .ok_or("サーバーが選択されていません")?;
        let os_username =
            extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_OS_USERNAME_INPUT)
                .ok_or("OSユーザー名が入力されていません")?
                .trim()
                .to_string();

        Ok(LinkTarget {
            external_system: ExternalSystem::Os {
                server: server.clone(),
            },
            display: format!("{}@{}", os_username, server),
            external_user_id: os_username,
        })
    } else {
        let target_user_id =
            extract_form_data::get_user_select(view_submission, ACTION_USER_SELECT)
                .ok_or("ユーザーが選択されていません")?;

        Ok(LinkTarget {
            external_system: ExternalSystem::Slack,
            display: format!("<@{}>", target_user_id),
            external_user_id: target_user_id,
        })
    }
}

/// ユーザーリンクモーダル送信を処理
///
/// 他のユーザー（SlackユーザーまたはOSユーザー名）をメールアドレスに紐付け、
/// カレンダーアクセス権を付与（管理者用）
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    debug!("handling identity link submission");

    let user_id = view_submission.user.id.clone();

    // リンク対象種別に応じてリンク対象を抽出
    let target_type =
        extract_form_data::get_selected_option_value(view_submission, ACTION_LINK_TARGET_TYPE);
    let link_target = extract_link_target(view_submission, target_type.as_deref())?;

    // メールアドレスを抽出
    let email_value =
        extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
            .ok_or("メールアドレスが入力されていません")?;

    // メールアドレスのバリデーション
    let email_result = EmailAddress::new(email_value.trim().to_string());

    // ユーザーをリンク
    let link_result = match &email_result {
        Ok(email) => app
            .usecases()
            .grant_access
            .execute(
                link_target.external_system.clone(),
                link_target.external_user_id.clone(),
                email.clone(),
            )
            .await
            .map_err(|e| e.into()),
        Err(e) => Err(Box::new(e.clone()) as Box<dyn std::error::Error + Send + Sync>),
    };

    // channel_id を取得
    let channel_id = app
        .user_channel_map()
        .read()
        .unwrap()
        .get(&user_id)
        .cloned()
        .ok_or("セッションの有効期限が切れました。もう一度コマンドを実行してください。")?;

    // エフェメラルメッセージで結果を送信
    let message_text = match link_result {
        Ok(_) => {
            info!(
                identity = %link_target.display,
                email = %email_result.as_ref().unwrap().as_str(),
                "identity linked"
            );
            format!(
                "✅ ユーザー {} をメールアドレス {} に紐付けました",
                link_target.display,
                email_result.as_ref().unwrap().as_str()
            )
        }
        Err(e) => {
            error!(error = %e, "linking the identity failed");
            format!("❌ 紐付けに失敗しました: {}", e)
        }
    };

    let ephemeral_req = SlackApiChatPostEphemeralRequest::new(
        channel_id,
        user_id.clone(),
        SlackMessageContent::new().with_text(message_text),
    );

    let session = app.slack_client().open_session(app.bot_token());
    session.chat_post_ephemeral(&ephemeral_req).await?;

    // モーダルを閉じる
    Ok(None)
}
