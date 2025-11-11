//! 結果表示モーダル
//!
//! 成功・失敗・処理中を表示するシンプルなモーダル

use slack_morphism::prelude::*;

/// 処理中モーダルを作成
///
/// # 引数
/// * `title` - タイトル
/// * `message` - メッセージ
pub fn create_processing_modal(title: impl Into<String>, message: impl Into<String>) -> SlackView {
    let title_str = title.into();
    let message_str = message.into();

    SlackView::Modal(SlackModalView {
        callback_id: Some("result_processing".into()),
        title: pt!(title_str),
        blocks: vec![SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!(format!("⏳ {}", message_str)),
        ))],
        close: Some(pt!("閉じる")),
        submit: None,
        private_metadata: None,
        clear_on_close: None,
        notify_on_close: None,
        external_id: None,
        hash: None,
    })
}

/// 成功結果モーダルを作成
///
/// # 引数
/// * `title` - タイトル
/// * `message` - メッセージ
pub fn create_success_modal(title: impl Into<String>, message: impl Into<String>) -> SlackView {
    let title_str = title.into();
    let message_str = message.into();

    SlackView::Modal(SlackModalView {
        callback_id: Some("result_success".into()),
        title: pt!(title_str),
        blocks: vec![SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!(format!("✅ {}", message_str)),
        ))],
        close: Some(pt!("閉じる")),
        submit: None,
        private_metadata: None,
        clear_on_close: None,
        notify_on_close: None,
        external_id: None,
        hash: None,
    })
}

/// エラー結果モーダルを作成
///
/// # 引数
/// * `title` - タイトル
/// * `error_message` - エラーメッセージ
pub fn create_error_modal(title: impl Into<String>, error_message: impl Into<String>) -> SlackView {
    let title_str = title.into();
    let error_str = error_message.into();

    SlackView::Modal(SlackModalView {
        callback_id: Some("result_error".into()),
        title: pt!(title_str),
        blocks: vec![SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!(format!("❌ {}", error_str)),
        ))],
        close: Some(pt!("閉じる")),
        submit: None,
        private_metadata: None,
        clear_on_close: None,
        notify_on_close: None,
        external_id: None,
        hash: None,
    })
}
