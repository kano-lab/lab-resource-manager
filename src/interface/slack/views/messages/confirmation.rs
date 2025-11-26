//! 確認メッセージブロック

use slack_morphism::prelude::*;

/// シンプルな確認メッセージを作成
///
/// # 引数
/// * `message` - 確認メッセージのテキスト
pub fn create_simple(message: impl Into<String>) -> SlackMessageContent {
    SlackMessageContent::new().with_text(format!("✅ {}", message.into()))
}

/// 詳細付き確認メッセージを作成
///
/// # 引数
/// * `title` - 確認メッセージのタイトル
/// * `details` - 追加の詳細情報
pub fn create_with_details(
    title: impl Into<String>,
    details: impl Into<String>,
) -> SlackMessageContent {
    let title_str = title.into();
    let details_str = details.into();

    SlackMessageContent::new()
        .with_text(format!("✅ {}", title_str))
        .with_blocks(vec![
            SlackBlock::Section(
                SlackSectionBlock::new().with_text(md!(format!("*✅ {}*", title_str))),
            ),
            SlackBlock::Section(SlackSectionBlock::new().with_text(md!(details_str))),
        ])
}

/// 確認メッセージを表示するモーダルを作成
///
/// # 引数
/// * `title` - モーダルのタイトル
/// * `message` - 確認メッセージ
pub fn create_confirmation_modal(
    title: impl Into<String>,
    message: impl Into<String>,
) -> SlackView {
    let blocks = vec![SlackBlock::Section(
        SlackSectionBlock::new().with_text(md!(format!("✅ {}", message.into()))),
    )];

    SlackView::Modal(SlackModalView::new(pt!(title.into()), blocks).with_close(pt!("閉じる")))
}
