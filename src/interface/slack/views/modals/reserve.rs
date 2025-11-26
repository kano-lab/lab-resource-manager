//! リソース予約モーダルビルダー

use crate::interface::slack::constants::{
    ACTION_END_TIME, ACTION_GPU_DEVICE_NUMBER, ACTION_GPU_MODEL, ACTION_GPU_SERVER,
    ACTION_NOTES, ACTION_RESOURCE_TYPE, ACTION_ROOM_NAME, ACTION_START_TIME, CALLBACK_RESERVE,
};
use slack_morphism::prelude::*;

/// リソース予約モーダルを作成
///
/// `/reserve` コマンドで使用される、
/// GPUまたは部屋のリソースを予約するモーダル
pub fn create() -> SlackView {
    let blocks = vec![
        SlackBlock::Section(
            SlackSectionBlock::new().with_text(md!(
                "リソースを予約します。\nリソースタイプを選択し、必要な情報を入力してください。"
            )),
        ),
        // リソースタイプ選択
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("リソースタイプ"),
                SlackInputBlockElement::StaticSelect(
                    SlackBlockStaticSelectElement::new(SlackActionId::new(
                        ACTION_RESOURCE_TYPE.to_string(),
                    ))
                    .with_placeholder(pt!("リソースタイプを選択"))
                    .with_options(vec![
                        SlackBlockChoiceItem {
                            text: pt!("GPU"),
                            value: "gpu".into(),
                            url: None,
                        },
                        SlackBlockChoiceItem {
                            text: pt!("部屋"),
                            value: "room".into(),
                            url: None,
                        },
                    ]),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_RESOURCE_TYPE.to_string())),
        ),
        SlackBlock::Divider(SlackDividerBlock::new()),
        // GPU用フィールド
        SlackBlock::Section(SlackSectionBlock::new().with_text(md!("*GPU情報*\nGPUを予約する場合は以下を入力してください"))),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("サーバー名"),
                SlackInputBlockElement::StaticSelect(
                    SlackBlockStaticSelectElement::new(SlackActionId::new(
                        ACTION_GPU_SERVER.to_string(),
                    ))
                    .with_placeholder(pt!("サーバーを選択"))
                    .with_options(vec![
                        SlackBlockChoiceItem {
                            text: pt!("Thalys"),
                            value: "Thalys".into(),
                            url: None,
                        },
                        SlackBlockChoiceItem {
                            text: pt!("Freccia"),
                            value: "Freccia".into(),
                            url: None,
                        },
                        SlackBlockChoiceItem {
                            text: pt!("Lyria"),
                            value: "Lyria".into(),
                            url: None,
                        },
                    ]),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_GPU_SERVER.to_string()))
            .with_optional(true),
        ),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("デバイス番号"),
                SlackInputBlockElement::PlainTextInput(
                    SlackBlockPlainTextInputElement::new(SlackActionId::new(
                        ACTION_GPU_DEVICE_NUMBER.to_string(),
                    ))
                    .with_placeholder(pt!("0, 1, 2, ...")),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_GPU_DEVICE_NUMBER.to_string()))
            .with_optional(true),
        ),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("GPUモデル"),
                SlackInputBlockElement::PlainTextInput(
                    SlackBlockPlainTextInputElement::new(SlackActionId::new(
                        ACTION_GPU_MODEL.to_string(),
                    ))
                    .with_placeholder(pt!("例: A100, RTX6000")),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_GPU_MODEL.to_string()))
            .with_optional(true),
        ),
        SlackBlock::Divider(SlackDividerBlock::new()),
        // 部屋用フィールド
        SlackBlock::Section(SlackSectionBlock::new().with_text(md!("*部屋情報*\n部屋を予約する場合は以下を入力してください"))),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("部屋名"),
                SlackInputBlockElement::PlainTextInput(
                    SlackBlockPlainTextInputElement::new(SlackActionId::new(
                        ACTION_ROOM_NAME.to_string(),
                    ))
                    .with_placeholder(pt!("例: 会議室A")),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_ROOM_NAME.to_string()))
            .with_optional(true),
        ),
        SlackBlock::Divider(SlackDividerBlock::new()),
        // 共通フィールド
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("開始日時"),
                SlackInputBlockElement::DateTimePicker(SlackBlockDateTimePickerElement::new(
                    SlackActionId::new(ACTION_START_TIME.to_string()),
                )),
            )
            .with_block_id(SlackBlockId::new(ACTION_START_TIME.to_string())),
        ),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("終了日時"),
                SlackInputBlockElement::DateTimePicker(SlackBlockDateTimePickerElement::new(
                    SlackActionId::new(ACTION_END_TIME.to_string()),
                )),
            )
            .with_block_id(SlackBlockId::new(ACTION_END_TIME.to_string())),
        ),
        SlackBlock::Input(
            SlackInputBlock::new(
                pt!("備考"),
                SlackInputBlockElement::PlainTextInput(
                    SlackBlockPlainTextInputElement::new(SlackActionId::new(
                        ACTION_NOTES.to_string(),
                    ))
                    .with_multiline(true)
                    .with_placeholder(pt!("任意: 予約の詳細や目的など")),
                ),
            )
            .with_block_id(SlackBlockId::new(ACTION_NOTES.to_string()))
            .with_optional(true),
        ),
    ];

    SlackView::Modal(
        SlackModalView::new(pt!("リソース予約"), blocks)
            .with_callback_id(CALLBACK_RESERVE.into())
            .with_submit(pt!("予約"))
            .with_close(pt!("キャンセル")),
    )
}
