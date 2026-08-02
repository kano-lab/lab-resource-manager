//! ボタンを押した利用者へ結果を返すための部品
//!
//! どこへ返すのかは、インタラクションのイベントの形によって置き場所が変わる。
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
