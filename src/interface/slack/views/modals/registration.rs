//! Email registration modal builder

use crate::interface::slack::constants::{ACTION_EMAIL_INPUT, CALLBACK_REGISTER_EMAIL};
use slack_morphism::prelude::*;

/// メールアドレス登録モーダルを作成
///
/// リソース予約前にGoogleカレンダー連携用のメールアドレスを登録するためのモーダル
pub fn create_register_email_modal() -> SlackView {
    let blocks = vec![
        SlackBlock::Section(
            SlackSectionBlock::new()
                .with_text(md!("リソースを予約するには、Googleカレンダーと連携するためのメールアドレスを登録する必要があります。"))
        ),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("メールアドレス"),
                SlackInputBlockElement::PlainTextInput(
                    SlackBlockPlainTextInputElement::new(
                        SlackActionId::new(ACTION_EMAIL_INPUT.to_string())
                    )
                    .with_placeholder(pt!("your-email@gmail.com"))
                )
            )
            .with_block_id(SlackBlockId::new(ACTION_EMAIL_INPUT.to_string()))
        ),
    ];

    SlackView::Modal(
        SlackModalView::new(
            pt!("メールアドレスの登録"),
            blocks,
        )
        .with_callback_id(CALLBACK_REGISTER_EMAIL.into())
        .with_submit(pt!("登録"))
        .with_close(pt!("キャンセル"))
    )
}
