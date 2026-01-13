//! ユーザーリンクモーダルビルダー

use crate::interface::slack::constants::{
    ACTION_LINK_EMAIL_INPUT, ACTION_USER_SELECT, CALLBACK_LINK_USER,
};
use slack_morphism::prelude::*;

/// ユーザーリンクモーダルを作成
///
/// `/link-user` コマンドで使用される、
/// 他のユーザーをメールアドレスに紐付けるモーダル（管理者用）
pub fn create() -> SlackView {
    let blocks = vec![
        SlackBlock::Section(
            SlackSectionBlock::new().with_text(md!(
                "他のユーザーをGoogleカレンダーのメールアドレスに紐付けます。\n紐付けられたユーザーに、カレンダーへのアクセス権が自動的に付与されます。"
            )),
        ),
        SlackBlock::Input(
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
        ),
        SlackBlock::Input(
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
        ),
    ];

    SlackView::Modal(
        SlackModalView::new(pt!("ユーザーをメールアドレスに紐付け"), blocks)
            .with_callback_id(CALLBACK_LINK_USER.into())
            .with_submit(pt!("紐付け"))
            .with_close(pt!("キャンセル")),
    )
}
