//! 確認メッセージブロック

use slack_morphism::prelude::*;

/// シンプルな確認メッセージを作成
///
/// # 引数
/// * `message` - Confirmation message text
pub fn create_simple(message: impl Into<String>) -> SlackMessageContent {
    SlackMessageContent::new().with_text(format!("✅ {}", message.into()))
}

/// 詳細付き確認メッセージを作成
///
/// # 引数
/// * `title` - Title of the confirmation
/// * `details` - Additional details
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
                SlackSectionBlock::new().with_text(md!(format!("*{}*", title_str))),
            ),
            SlackBlock::Section(SlackSectionBlock::new().with_text(md!(details_str))),
        ])
}
