//! フォームデータ抽出
//!
//! Slackモーダルからフォーム値を抽出するユーティリティ

use slack_morphism::prelude::*;

/// ビュー送信からプレーンテキスト入力値を取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_plain_text_input(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.value.as_ref().map(|s| s.to_string());
            }
        }
    }
    None
}

/// ラジオボタンまたはセレクトメニューから選択されたオプション値を取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_selected_option_value(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.selected_option.as_ref().map(|opt| opt.value.clone());
            }
        }
    }
    None
}

/// ラジオボタンまたはセレクトメニューから選択されたオプションのテキストを取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_selected_option_text(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value
                    .selected_option
                    .as_ref()
                    .map(|opt| opt.text.text.clone());
            }
        }
    }
    None
}

/// 選択された日付を取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_selected_date(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.selected_date.as_ref().map(|d| d.to_string());
            }
        }
    }
    None
}

/// 選択された時刻を取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_selected_time(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.selected_time.as_ref().map(|t| t.to_string());
            }
        }
    }
    None
}

/// 選択された日時を取得（DateTimePickerから）
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
///
/// # 戻り値
/// UNIXタイムスタンプ（秒）
pub fn get_selected_datetime(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<i64> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.selected_date_time.as_ref().map(|dt| dt.0.timestamp());
            }
        }
    }
    None
}

/// 複数選択されたオプションの値を取得（チェックボックスまたはマルチセレクトから）
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
///
/// # 戻り値
/// 選択されたオプションの値のリスト（表示テキストではなく値）
pub fn get_selected_options(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Vec<String> {
    let Some(state) = &view_submission.view.state_params.state else {
        return Vec::new();
    };
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value
                    .selected_options
                    .as_ref()
                    .map(|options| options.iter().map(|opt| opt.value.clone()).collect())
                    .unwrap_or_default();
            }
        }
    }
    Vec::new()
}

/// モーダルビューからprivate_metadataを取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
pub fn get_private_metadata(
    view_submission: &SlackInteractionViewSubmissionEvent,
) -> Option<String> {
    let SlackView::Modal(modal_view) = &view_submission.view.view else {
        return None;
    };

    modal_view.private_metadata.as_ref().map(|s| s.to_string())
}

/// ユーザー選択から選択されたユーザーIDを取得
///
/// # 引数
/// * `view_submission` - ビュー送信イベント
/// * `action_id_str` - アクションID文字列
pub fn get_user_select(
    view_submission: &SlackInteractionViewSubmissionEvent,
    action_id_str: &str,
) -> Option<String> {
    let state = view_submission.view.state_params.state.as_ref()?;
    let values = &state.values;

    for (_block_id, actions_map) in values.iter() {
        for (action_id, value) in actions_map.iter() {
            if action_id.to_string() == action_id_str {
                return value.selected_user.as_ref().map(|u| u.to_string());
            }
        }
    }
    None
}
