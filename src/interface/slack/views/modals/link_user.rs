//! ユーザーリンクモーダルビルダー

use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::constants::{
    ACTION_LINK_EMAIL_INPUT, ACTION_LINK_OS_USERNAME_INPUT, ACTION_LINK_SERVER_SELECT,
    ACTION_LINK_TARGET_TYPE, ACTION_USER_SELECT, CALLBACK_LINK_USER,
};
use slack_morphism::prelude::*;

/// ユーザーリンクモーダルを作成
///
/// `/link-user` コマンドで使用される、
/// 他のユーザーをメールアドレスに紐付けるモーダル（管理者用）。
/// リンク対象は「Slackユーザー」または「OSユーザー名」から選択できる。
///
/// # 引数
/// * `config` - リソース設定（OSユーザー名リンク時のサーバー選択肢に使用）
/// * `target_type` - 選択中のリンク対象種別（"slack" or "os"、デフォルトは"slack"）
pub fn create(config: &ResourceConfig, target_type: Option<&str>) -> SlackView {
    let current_target_type = target_type.unwrap_or("slack");

    let target_type_options = vec![
        SlackBlockChoiceItem::new(pt!("Slackユーザー"), "slack".into()),
        SlackBlockChoiceItem::new(pt!("OSユーザー名"), "os".into()),
    ];

    let initial_target_type = if current_target_type == "os" {
        SlackBlockChoiceItem::new(pt!("OSユーザー名"), "os".into())
    } else {
        SlackBlockChoiceItem::new(pt!("Slackユーザー"), "slack".into())
    };

    let mut blocks: Vec<SlackBlock> = vec![SlackBlock::Section(
        SlackSectionBlock::new().with_text(md!(
            "他のユーザーをGoogleカレンダーのメールアドレスに紐付けます。\n紐付けられたユーザーに、カレンダーへのアクセス権が自動的に付与されます。"
        )),
    )];

    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("リンク対象"),
            SlackInputBlockElement::RadioButtons(
                SlackBlockRadioButtonsElement::new(
                    SlackActionId::new(ACTION_LINK_TARGET_TYPE.to_string()),
                    target_type_options,
                )
                .with_initial_option(initial_target_type),
            ),
        )
        .with_dispatch_action(true),
    ));

    if current_target_type == "os" {
        add_os_username_blocks(&mut blocks, config);
    } else {
        add_slack_user_blocks(&mut blocks);
    }

    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("メールアドレス"),
            SlackInputBlockElement::PlainTextInput(
                SlackBlockPlainTextInputElement::new(SlackActionId::new(
                    ACTION_LINK_EMAIL_INPUT.to_string(),
                ))
                .with_placeholder(pt!("user@gmail.com")),
            ),
        )
        .with_block_id(SlackBlockId::new(ACTION_LINK_EMAIL_INPUT.to_string())),
    ));

    SlackView::Modal(
        SlackModalView::new(pt!("ユーザーをメールアドレスに紐付け"), blocks)
            .with_callback_id(CALLBACK_LINK_USER.into())
            .with_submit(pt!("紐付け"))
            .with_close(pt!("キャンセル")),
    )
}

/// Slackユーザー選択ブロックを追加
fn add_slack_user_blocks(blocks: &mut Vec<SlackBlock>) {
    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("紐付けるユーザー"),
            SlackInputBlockElement::UsersSelect(
                SlackBlockUsersSelectElement::new(SlackActionId::new(
                    ACTION_USER_SELECT.to_string(),
                ))
                .with_placeholder(pt!("ユーザーを選択")),
            ),
        )
        .with_block_id(SlackBlockId::new(ACTION_USER_SELECT.to_string())),
    ));
}

/// OSユーザー名リンク用のサーバー選択（複数選択可）・ユーザー名入力ブロックを追加
///
/// 選択した全サーバーに、同じOSユーザー名でメールアドレスを紐付ける。
/// サーバーごとにユーザー名が異なる場合は、ユーザー名が共通するサーバーだけを選んで
/// 複数回に分けて実行する。
fn add_os_username_blocks(blocks: &mut Vec<SlackBlock>, config: &ResourceConfig) {
    if config.servers.is_empty() {
        blocks.push(SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!("⚠️ サーバー設定が見つかりません。管理者に問い合わせてください。"),
        )));
        return;
    }

    let server_options: Vec<SlackBlockChoiceItem<SlackBlockText>> = config
        .servers
        .iter()
        .map(|server| {
            SlackBlockChoiceItem::new(
                SlackBlockText::Plain(SlackBlockPlainText::from(server.name.clone())),
                server.name.clone(),
            )
        })
        .collect();

    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("サーバー（複数選択可、同じユーザー名で一括リンク）"),
        SlackInputBlockElement::Checkboxes(SlackBlockCheckboxesElement::new(
            SlackActionId::new(ACTION_LINK_SERVER_SELECT.to_string()),
            server_options,
        )),
    )));

    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("OSユーザー名"),
            SlackInputBlockElement::PlainTextInput(
                SlackBlockPlainTextInputElement::new(SlackActionId::new(
                    ACTION_LINK_OS_USERNAME_INPUT.to_string(),
                ))
                .with_placeholder(pt!("kkawaguchi")),
            ),
        )
        .with_block_id(SlackBlockId::new(ACTION_LINK_OS_USERNAME_INPUT.to_string())),
    ));
}
