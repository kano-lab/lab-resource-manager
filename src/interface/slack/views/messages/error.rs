//! エラーメッセージブロック

use slack_morphism::prelude::*;

/// シンプルなエラーメッセージを作成
///
/// # 引数
/// * `message` - エラーメッセージのテキスト
pub fn create_simple(message: impl Into<String>) -> SlackMessageContent {
    SlackMessageContent::new().with_text(format!("❌ {}", message.into()))
}

/// 詳細付きエラーメッセージを作成
///
/// # 引数
/// * `title` - エラーのタイトル
/// * `details` - 追加のエラー詳細情報
pub fn create_with_details(
    title: impl Into<String>,
    details: impl Into<String>,
) -> SlackMessageContent {
    let title_str = title.into();
    let details_str = details.into();

    SlackMessageContent::new()
        .with_text(format!("❌ {}", title_str))
        .with_blocks(vec![
            SlackBlock::Section(
                SlackSectionBlock::new().with_text(md!(format!("*❌ {}*", title_str))),
            ),
            SlackBlock::Section(SlackSectionBlock::new().with_text(md!(details_str))),
        ])
}

/// エラーメッセージを表示するモーダルを作成
///
/// # 引数
/// * `title` - モーダルのタイトル
/// * `message` - エラーメッセージ
pub fn create_error_modal(title: impl Into<String>, message: impl Into<String>) -> SlackView {
    let blocks = vec![SlackBlock::Section(
        SlackSectionBlock::new().with_text(md!(format!("❌ {}", message.into()))),
    )];

    SlackView::Modal(SlackModalView::new(pt!(title.into()), blocks).with_close(pt!("閉じる")))
}
