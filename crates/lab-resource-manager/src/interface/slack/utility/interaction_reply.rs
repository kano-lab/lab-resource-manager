//! ボタンを押した利用者へ結果を返すための部品
//!
//! 返信先・押されたメッセージ・置き換える中身という、返信に必要な3つを扱う。
//! どこへ返すのかはインタラクションのイベントの形によって置き場所が変わるため、
//! 各ハンドラがその差を意識しなくて済むよう、ここで吸収する。

use slack_morphism::prelude::*;

/// 結果を返す先のチャンネル
///
/// インタラクションのイベントは、チャンネルを`channel`で伝えることも、
/// ボタンが乗っていたメッセージの一部として伝えることもある。
pub fn channel_id(block_actions: &SlackInteractionBlockActionsEvent) -> Option<SlackChannelId> {
    if let Some(channel) = &block_actions.channel {
        return Some(channel.id.clone());
    }
    if let SlackInteractionActionContainer::Message(message) = &block_actions.container {
        return message.channel_id.clone();
    }
    None
}

/// ボタンが乗っているメッセージの位置
pub fn message_ref(
    block_actions: &SlackInteractionBlockActionsEvent,
) -> Option<(SlackChannelId, SlackTs)> {
    let SlackInteractionActionContainer::Message(message) = &block_actions.container else {
        return None;
    };
    let channel_id = message
        .channel_id
        .clone()
        .or_else(|| block_actions.channel.as_ref().map(|c| c.id.clone()))?;

    Some((channel_id, message.message_ts.clone()))
}

/// 操作が済んだあとのメッセージの中身
///
/// `chat.update`は渡さなかったフィールドをそのまま残すため、`text`だけを送っても
/// 送信時の`blocks`が生き続けてボタンは押せてしまう。結果だけを載せた`blocks`で
/// 上書きして初めてボタンが消える。
pub fn settled_message(feedback: String) -> SlackMessageContent {
    let block = SlackSectionBlock::new().with_text(md!(feedback.clone()));
    SlackMessageContent::new()
        .with_text(feedback)
        .with_blocks(slack_blocks![some_into(block)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settled_message_carries_blocks_without_any_button() {
        let content = settled_message("✅ 予約を作成しました".to_string());
        let json = serde_json::to_value(&content).unwrap();

        let blocks = json["blocks"]
            .as_array()
            .expect("blocksを送らないと送信時のボタンが残る");
        assert!(
            blocks.iter().all(|block| block["type"] != "actions"),
            "操作後のメッセージにボタンが残っている: {:?}",
            blocks
        );
        assert!(
            serde_json::to_string(&json)
                .unwrap()
                .contains("予約を作成しました"),
            "操作の結果が本文に出ていない: {:?}",
            json
        );
    }
}
