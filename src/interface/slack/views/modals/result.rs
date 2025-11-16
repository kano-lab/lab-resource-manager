//! 処理結果モーダル（成功/失敗）

use slack_morphism::prelude::*;

/// 成功モーダルを作成
///
/// # Arguments
/// * `title` - モーダルのタイトル
/// * `message` - 表示するメッセージ
pub fn create_success(title: &str, message: &str) -> SlackView {
    let blocks = vec![SlackBlock::Section(
        SlackSectionBlock::new()
            .with_text(md!(format!("✅ {}", message)))
            .with_block_id("success_message".into()),
    )];

    SlackView::Modal(
        SlackModalView::new(pt!(title), blocks)
            .with_close(pt!("閉じる")),
    )
}

/// 失敗モーダルを作成
///
/// # Arguments
/// * `title` - モーダルのタイトル
/// * `error_message` - エラーメッセージ
pub fn create_error(title: &str, error_message: &str) -> SlackView {
    let blocks = vec![SlackBlock::Section(
        SlackSectionBlock::new()
            .with_text(md!(format!("❌ {}", error_message)))
            .with_block_id("error_message".into()),
    )];

    SlackView::Modal(
        SlackModalView::new(pt!(title), blocks)
            .with_close(pt!("閉じる")),
    )
}
