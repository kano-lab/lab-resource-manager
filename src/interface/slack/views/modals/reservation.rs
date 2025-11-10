//! Resource reservation modal builder

use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::constants::*;
use chrono::{Local, Timelike};
use slack_morphism::prelude::*;

/// 予約作成・更新用のモーダルを作成
///
/// # Arguments
/// * `config` - リソース設定
/// * `resource_type` - 選択されたリソースタイプ ("gpu" or "room")
/// * `selected_server` - 選択されたサーバー名（GPU選択時のみ）
/// * `usage_id` - 更新対象の予約ID（Noneの場合は新規作成）
///
/// # Returns
/// 予約フォームのモーダルビュー
pub fn create_reserve_modal(
    config: &ResourceConfig,
    resource_type: Option<&str>,
    selected_server: Option<&str>,
    usage_id: Option<&str>,
) -> SlackView {
    // 現在時刻を取得してデフォルト値を設定
    let now = Local::now();
    let start_date = now.format("%Y-%m-%d").to_string();
    let start_time = format!("{:02}:{:02}", now.hour(), now.minute());

    let end = now + chrono::Duration::hours(1);
    let end_date = end.format("%Y-%m-%d").to_string();
    let end_time = format!("{:02}:{:02}", end.hour(), end.minute());

    // 現在選択中のリソースタイプ (デフォルトは "gpu")
    let current_resource_type = resource_type.unwrap_or("gpu");

    // リソースタイプ選択肢（GPU or Room）
    let resource_type_options = vec![
        SlackBlockChoiceItem::new(pt!("GPU Server"), "gpu".into()),
        SlackBlockChoiceItem::new(pt!("Room"), "room".into()),
    ];

    // 初期選択値を決定
    let initial_resource_type = if current_resource_type == "room" {
        SlackBlockChoiceItem::new(pt!("Room"), "room".into())
    } else {
        SlackBlockChoiceItem::new(pt!("GPU Server"), "gpu".into())
    };

    // モーダルのブロックを動的に構築
    let mut blocks: Vec<SlackBlock> = vec![];

    // リソースタイプ選択（常に表示）
    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("リソースタイプ"),
            SlackInputBlockElement::RadioButtons(
                SlackBlockRadioButtonsElement::new(
                    SlackActionId::new(ACTION_RESERVE_RESOURCE_TYPE.to_string()),
                    resource_type_options,
                )
                .with_initial_option(initial_resource_type),
            ),
        )
        .with_dispatch_action(true),
    ));

    // リソースタイプに応じて条件分岐
    if current_resource_type == "gpu" {
        add_gpu_blocks(&mut blocks, config, selected_server);
    } else if current_resource_type == "room" {
        add_room_blocks(&mut blocks, config);
    }

    // 日時フィールド（常に表示）
    add_datetime_blocks(&mut blocks, &start_date, &start_time, &end_date, &end_time);

    // 備考（常に表示、オプション）
    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("備考"),
            SlackInputBlockElement::PlainTextInput(
                SlackBlockPlainTextInputElement::new(SlackActionId::new(
                    ACTION_RESERVE_NOTES.to_string(),
                ))
                .with_multiline(true),
            ),
        )
        .with_optional(true),
    ));

    // モーダルの作成（usage_idがあれば更新、なければ作成）
    let (callback_id, title, submit_text) = if usage_id.is_some() {
        (CALLBACK_UPDATE_SUBMIT, "リソース予約を更新", "更新する")
    } else {
        (CALLBACK_RESERVE_SUBMIT, "リソース予約", "予約する")
    };

    let mut modal_view = SlackModalView::new(pt!(title), blocks)
        .with_callback_id(callback_id.into())
        .with_submit(pt!(submit_text))
        .with_close(pt!("キャンセル"));

    // usage_idがあればprivate_metadataに設定
    if let Some(id) = usage_id {
        modal_view = modal_view.with_private_metadata(id.into());
    }

    SlackView::Modal(modal_view)
}

/// GPUサーバー選択ブロックを追加
fn add_gpu_blocks(
    blocks: &mut Vec<SlackBlock>,
    config: &ResourceConfig,
    selected_server: Option<&str>,
) {
    // GPU Server選択肢
    let server_options: Vec<SlackBlockChoiceItem<SlackBlockPlainTextOnly>> = config
        .servers
        .iter()
        .map(|server| SlackBlockChoiceItem::new(pt!(server.name.clone()), server.name.clone()))
        .collect();

    // GPU Server選択フィールド
    let mut server_select_element = SlackBlockStaticSelectElement::new(SlackActionId::new(
        ACTION_RESERVE_SERVER_SELECT.to_string(),
    ))
    .with_placeholder(pt!("サーバーを選択"))
    .with_options(server_options.clone());

    // デフォルト値を設定
    // 既に選択されている場合はそれを、そうでない場合は最初のサーバーを選択
    let default_server_name =
        selected_server.or_else(|| config.servers.first().map(|s| s.name.as_str()));

    if let Some(server_name) = default_server_name {
        let initial_server =
            SlackBlockChoiceItem::new(pt!(server_name.to_string()), server_name.to_string());
        server_select_element = server_select_element.with_initial_option(initial_server);
    }

    blocks.push(SlackBlock::Input(
        SlackInputBlock::new(
            pt!("GPU Server"),
            SlackInputBlockElement::StaticSelect(server_select_element),
        )
        .with_dispatch_action(true),
    ));

    // デバイス選択肢（選択されたサーバーに応じて変更）
    let device_options: Vec<SlackBlockChoiceItem<SlackBlockText>> =
        if let Some(server_name) = selected_server {
            // 特定のサーバーのデバイスを表示
            config
                .servers
                .iter()
                .find(|s| s.name == server_name)
                .map(|server| {
                    server
                        .devices
                        .iter()
                        .map(|device| {
                            SlackBlockChoiceItem::new(
                                SlackBlockText::Plain(SlackBlockPlainText::from(format!(
                                    "Device {} ({})",
                                    device.id, device.model
                                ))),
                                device.id.to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            // サーバー未選択の場合は最初のサーバーのデバイスを表示
            config
                .servers
                .first()
                .map(|server| {
                    server
                        .devices
                        .iter()
                        .map(|device| {
                            SlackBlockChoiceItem::new(
                                SlackBlockText::Plain(SlackBlockPlainText::from(format!(
                                    "Device {} ({})",
                                    device.id, device.model
                                ))),
                                device.id.to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

    // GPU Device選択（チェックボックス）
    if !device_options.is_empty() {
        blocks.push(SlackBlock::Input(
            SlackInputBlock::new(
                pt!("GPU Devices"),
                SlackInputBlockElement::Checkboxes(SlackBlockCheckboxesElement::new(
                    SlackActionId::new(ACTION_RESERVE_DEVICES.to_string()),
                    device_options,
                )),
            )
            .with_optional(true),
        ));
    }
}

/// Room選択ブロックを追加
fn add_room_blocks(blocks: &mut Vec<SlackBlock>, config: &ResourceConfig) {
    // Room選択肢
    let room_options: Vec<SlackBlockChoiceItem<SlackBlockPlainTextOnly>> = config
        .rooms
        .iter()
        .map(|room| SlackBlockChoiceItem::new(pt!(room.name.clone()), room.name.clone()))
        .collect();

    // Room選択フィールド
    let mut room_select_element = SlackBlockStaticSelectElement::new(SlackActionId::new(
        ACTION_RESERVE_ROOM_SELECT.to_string(),
    ))
    .with_placeholder(pt!("部屋を選択"))
    .with_options(room_options.clone());

    // デフォルト値を設定（最初の部屋を選択）
    if let Some(first_room) = config.rooms.first() {
        let initial_room =
            SlackBlockChoiceItem::new(pt!(first_room.name.clone()), first_room.name.clone());
        room_select_element = room_select_element.with_initial_option(initial_room);
    }

    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("Room"),
        SlackInputBlockElement::StaticSelect(room_select_element),
    )));
}

/// 日時選択ブロックを追加
fn add_datetime_blocks(
    blocks: &mut Vec<SlackBlock>,
    start_date: &str,
    start_time: &str,
    end_date: &str,
    end_time: &str,
) {
    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("開始日"),
        SlackInputBlockElement::DatePicker(
            SlackBlockDatePickerElement::new(SlackActionId::new(
                ACTION_RESERVE_START_DATE.to_string(),
            ))
            .with_initial_date(start_date.to_string()),
        ),
    )));

    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("開始時刻"),
        SlackInputBlockElement::TimePicker(
            SlackBlockTimePickerElement::new(SlackActionId::new(
                ACTION_RESERVE_START_TIME.to_string(),
            ))
            .with_initial_time(start_time.to_string()),
        ),
    )));

    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("終了日"),
        SlackInputBlockElement::DatePicker(
            SlackBlockDatePickerElement::new(SlackActionId::new(
                ACTION_RESERVE_END_DATE.to_string(),
            ))
            .with_initial_date(end_date.to_string()),
        ),
    )));

    blocks.push(SlackBlock::Input(SlackInputBlock::new(
        pt!("終了時刻"),
        SlackInputBlockElement::TimePicker(
            SlackBlockTimePickerElement::new(SlackActionId::new(
                ACTION_RESERVE_END_TIME.to_string(),
            ))
            .with_initial_time(end_time.to_string()),
        ),
    )));
}
