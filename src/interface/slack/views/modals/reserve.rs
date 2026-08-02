//! リソース予約モーダルビルダー

use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::constants::*;
use chrono::{Local, Timelike};
use slack_morphism::prelude::*;

/// 予約モーダルの初期状態
///
/// 何も指定しなければ、新規予約のモーダルが既定の見出しと文言で開く。
#[derive(Debug, Default, Clone)]
pub struct ReserveModalParams<'a> {
    /// 選択されたリソースタイプ ("gpu" or "room")
    pub resource_type: Option<&'a str>,
    /// 選択されたサーバー名（GPU選択時のみ）
    pub selected_server: Option<&'a str>,
    /// あらかじめ選んでおくデバイス番号（GPU選択時のみ）
    pub selected_devices: &'a [u32],
    /// 更新対象の予約ID（Noneの場合は新規作成）
    pub usage_id: Option<&'a str>,
    /// モーダルのコールバックID（デフォルト: "reserve_submit"）
    pub callback_id: Option<&'a str>,
    /// モーダルのタイトル（デフォルト: "リソース予約"）
    pub title: Option<&'a str>,
    /// 送信ボタンのテキスト（デフォルト: "予約する"）
    pub submit_text: Option<&'a str>,
}

/// 予約作成・更新用のモーダルを作成
///
/// # 引数
/// * `config` - リソース設定
/// * `params` - モーダルの初期状態
///
/// # 戻り値
/// 予約フォームのモーダルビュー
pub fn create_reserve_modal(config: &ResourceConfig, params: &ReserveModalParams) -> SlackView {
    let ReserveModalParams {
        resource_type,
        selected_server,
        selected_devices,
        usage_id,
        callback_id,
        title,
        submit_text,
    } = *params;

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
        add_gpu_blocks(&mut blocks, config, selected_server, selected_devices);
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

    // モーダルの作成
    let callback_id = callback_id.unwrap_or(CALLBACK_RESERVE_SUBMIT);
    let title = title.unwrap_or("リソース予約");
    let submit_text = submit_text.unwrap_or("予約する");

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

/// サーバーのデバイスリストから選択肢を生成
fn create_device_options(
    server: &crate::infrastructure::config::resource_config::ServerConfig,
) -> Vec<SlackBlockChoiceItem<SlackBlockText>> {
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
}

/// GPUサーバー選択ブロックを追加
fn add_gpu_blocks(
    blocks: &mut Vec<SlackBlock>,
    config: &ResourceConfig,
    selected_server: Option<&str>,
    selected_devices: &[u32],
) {
    // サーバー設定が空の場合はエラーメッセージを表示
    if config.servers.is_empty() {
        blocks.push(SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!("⚠️ サーバー設定が見つかりません。管理者に問い合わせてください。"),
        )));
        return;
    }

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
                .map(create_device_options)
                .unwrap_or_default()
        } else {
            // サーバー未選択の場合は最初のサーバーのデバイスを表示
            config
                .servers
                .first()
                .map(create_device_options)
                .unwrap_or_default()
        };

    // GPU Device選択（チェックボックス）
    if !device_options.is_empty() {
        let mut devices_element = SlackBlockCheckboxesElement::new(
            SlackActionId::new(ACTION_RESERVE_DEVICES.to_string()),
            device_options.clone(),
        );

        // あらかじめ選んでおくデバイスは、表示中の選択肢に含まれるものだけを選ぶ
        // （別サーバーの番号が渡ってもSlackが選択肢と対応づけられないため）
        let initial_devices: Vec<SlackBlockChoiceItem<SlackBlockText>> = device_options
            .into_iter()
            .filter(|option| {
                selected_devices
                    .iter()
                    .any(|number| number.to_string() == option.value)
            })
            .collect();

        if !initial_devices.is_empty() {
            devices_element = devices_element.with_initial_options(initial_devices);
        }

        blocks.push(SlackBlock::Input(
            SlackInputBlock::new(
                pt!("GPU Devices"),
                SlackInputBlockElement::Checkboxes(devices_element),
            )
            .with_optional(true),
        ));
    }
}

/// Room選択ブロックを追加
fn add_room_blocks(blocks: &mut Vec<SlackBlock>, config: &ResourceConfig) {
    // 部屋設定が空の場合はエラーメッセージを表示
    if config.rooms.is_empty() {
        blocks.push(SlackBlock::Section(SlackSectionBlock::new().with_text(
            md!("⚠️ 部屋設定が見つかりません。管理者に問い合わせてください。"),
        )));
        return;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::{DeviceConfig, ServerConfig};

    fn config() -> ResourceConfig {
        ResourceConfig {
            servers: vec![
                ServerConfig {
                    name: "gpu-server-1".to_string(),
                    calendar_id: "cal-1".to_string(),
                    devices: (0..4)
                        .map(|id| DeviceConfig {
                            id,
                            model: "A100 80GB PCIe".to_string(),
                        })
                        .collect(),
                    notifications: vec![],
                },
                ServerConfig {
                    name: "gpu-server-2".to_string(),
                    calendar_id: "cal-2".to_string(),
                    devices: vec![DeviceConfig {
                        id: 0,
                        model: "A100 80GB PCIe".to_string(),
                    }],
                    notifications: vec![],
                },
            ],
            rooms: vec![],
        }
    }

    /// デバイス選択チェックボックスの、あらかじめ選ばれている値を取り出す
    fn initially_checked_devices(view: &SlackView) -> Vec<String> {
        let SlackView::Modal(modal) = view else {
            panic!("モーダルが返るはず");
        };

        modal
            .blocks
            .iter()
            .find_map(|block| match block {
                SlackBlock::Input(input) => match &input.element {
                    SlackInputBlockElement::Checkboxes(checkboxes) => {
                        Some(checkboxes.initial_options.clone().unwrap_or_default())
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .map(|option| option.value)
            .collect()
    }

    #[test]
    fn nothing_is_checked_when_no_devices_are_given() {
        let view = create_reserve_modal(&config(), &ReserveModalParams::default());

        assert!(initially_checked_devices(&view).is_empty());
    }

    #[test]
    fn the_given_devices_come_up_already_checked() {
        let view = create_reserve_modal(
            &config(),
            &ReserveModalParams {
                selected_server: Some("gpu-server-1"),
                selected_devices: &[0, 3],
                ..Default::default()
            },
        );

        assert_eq!(initially_checked_devices(&view), vec!["0", "3"]);
    }

    #[test]
    fn a_device_the_shown_server_does_not_have_is_left_alone() {
        let view = create_reserve_modal(
            &config(),
            &ReserveModalParams {
                selected_server: Some("gpu-server-2"),
                selected_devices: &[0, 3],
                ..Default::default()
            },
        );

        assert_eq!(
            initially_checked_devices(&view),
            vec!["0"],
            "表示中のサーバーにない番号を選ぼうとすると、Slackが選択肢と対応づけられない"
        );
    }
}
