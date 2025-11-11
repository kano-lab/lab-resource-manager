//! エラーメッセージブロック

use slack_morphism::prelude::*;

/// シンプルなエラーメッセージを作成
///
/// # 引数
/// * `message` - Error message text
pub fn create_simple(message: impl Into<String>) -> SlackMessageContent {
    SlackMessageContent::new().with_text(format!("❌ {}", message.into()))
}

/// 詳細付きエラーメッセージを作成
///
/// # 引数
/// * `title` - Title of the error
/// * `details` - Additional error details
pub fn create_with_details(title: impl Into<String>, details: impl Into<String>) -> SlackMessageContent {
    let title_str = title.into();
    let details_str = details.into();

    SlackMessageContent::new()
        .with_text(format!("❌ {}", title_str))
        .with_blocks(vec![
            SlackBlock::Section(
                SlackSectionBlock::new()
                    .with_text(md!(format!("*❌ {}*", title_str)))
            ),
            SlackBlock::Section(
                SlackSectionBlock::new()
                    .with_text(md!(details_str))
            ),
        ])
}
