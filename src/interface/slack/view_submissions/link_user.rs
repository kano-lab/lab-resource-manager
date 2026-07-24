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
use tracing::{error, info};

/// リンク対象（外部システム上のユーザー識別情報）
struct LinkTarget {
    external_system: ExternalSystem,
    external_user_id: String,
    /// 結果メッセージ表示用のラベル（Slackなら`<@id>`、OSなら`username@server`）
    display: String,
}

/// フォームからリンク対象を抽出する
///
/// `target_type`が"os"の場合は選択された全サーバーに対して同じOSユーザー名で
/// リンク対象を1つずつ作る（複数サーバーへの一括紐付け）。
/// それ以外（デフォルト="slack"）の場合はSlackユーザー選択から1件だけ抽出する。
fn extract_link_targets(
    view_submission: &SlackInteractionViewSubmissionEvent,
    target_type: Option<&str>,
) -> Result<Vec<LinkTarget>, Box<dyn std::error::Error + Send + Sync>> {
    if target_type == Some("os") {
        let servers =
            extract_form_data::get_selected_options(view_submission, ACTION_LINK_SERVER_SELECT);
        if servers.is_empty() {
            return Err("サーバーが選択されていません".into());
        }
        let os_username =
            extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_OS_USERNAME_INPUT)
                .ok_or("OSユーザー名が入力されていません")?
                .trim()
                .to_string();

        Ok(servers
            .into_iter()
            .map(|server| LinkTarget {
                external_system: ExternalSystem::Os {
                    server: server.clone(),
                },
                display: format!("{}@{}", os_username, server),
                external_user_id: os_username.clone(),
            })
            .collect())
    } else {
        let target_user_id =
            extract_form_data::get_user_select(view_submission, ACTION_USER_SELECT)
                .ok_or("ユーザーが選択されていません")?;

        Ok(vec![LinkTarget {
            external_system: ExternalSystem::Slack,
            display: format!("<@{}>", target_user_id),
            external_user_id: target_user_id,
        }])
    }
}

/// ユーザーリンクモーダル送信を処理
///
/// 他のユーザー（SlackユーザーまたはOSユーザー名、OSユーザー名は複数サーバーへの
/// 一括紐付けが可能）をメールアドレスに紐付け、カレンダーアクセス権を付与（管理者用）
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Result<Option<SlackViewSubmissionResponse>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    info!("ユーザーリンクを処理中...");

    let user_id = view_submission.user.id.clone();

    // リンク対象種別に応じてリンク対象（1件以上）を抽出
    let target_type =
        extract_form_data::get_selected_option_value(view_submission, ACTION_LINK_TARGET_TYPE);
    let link_targets = extract_link_targets(view_submission, target_type.as_deref())?;

    // メールアドレスを抽出
    let email_value =
        extract_form_data::get_plain_text_input(view_submission, ACTION_LINK_EMAIL_INPUT)
            .ok_or("メールアドレスが入力されていません")?;

    // メールアドレスのバリデーション
    let email_result = EmailAddress::new(email_value.trim().to_string());

    // channel_id を取得
    let channel_id = app
        .user_channel_map()
        .read()
        .unwrap()
        .get(&user_id)
        .cloned()
        .ok_or("セッションの有効期限が切れました。もう一度コマンドを実行してください。")?;

    let message_text = match &email_result {
        Ok(email) => {
            // 同一メールアドレスのIdentityLinkを順次更新するため、リンク対象は逐次処理する
            // （ベストエフォート: 1件の失敗が他のサーバーへのリンクをブロックしない）
            let mut results = Vec::with_capacity(link_targets.len());
            for target in &link_targets {
                let result = app
                    .grant_access_usecase()
                    .execute(
                        target.external_system.clone(),
                        target.external_user_id.clone(),
                        email.clone(),
                    )
                    .await;
                results.push((target, result));
            }
            build_result_message(email.as_str(), &results)
        }
        Err(e) => {
            error!("❌ ユーザーリンクに失敗: {}", e);
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

/// リンク対象ごとの実行結果からエフェメラルメッセージを組み立てる
fn build_result_message(
    email: &str,
    results: &[(
        &LinkTarget,
        Result<(), crate::application::error::ApplicationError>,
    )],
) -> String {
    let lines: Vec<String> = results
        .iter()
        .map(|(target, result)| match result {
            Ok(_) => {
                info!("✅ ユーザーリンク成功: {} -> {}", target.display, email);
                format!("✅ {}", target.display)
            }
            Err(e) => {
                error!(
                    "❌ ユーザーリンクに失敗: {} -> {}: {}",
                    target.display, email, e
                );
                format!("❌ {}: {}", target.display, e)
            }
        })
        .collect();

    format!(
        "メールアドレス {} への紐付け結果:\n{}",
        email,
        lines.join("\n")
    )
}
