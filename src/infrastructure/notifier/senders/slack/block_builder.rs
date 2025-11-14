//! Slack Block Kit構築機能

use serde_json::json;
use slack_morphism::prelude::*;

/// Slack Block Kit構築器
pub struct SlackBlockBuilder;

impl SlackBlockBuilder {
    /// ボタン付きメッセージブロックを構築（JSON形式）
    pub fn build_message_with_buttons(message: &str, usage_id: &str) -> serde_json::Value {
        json!([
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": message
                }
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "🔄 更新"
                        },
                        "style": "primary",
                        "action_id": "edit_reservation",
                        "value": usage_id
                    },
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "❌ キャンセル"
                        },
                        "style": "danger",
                        "action_id": "cancel_reservation",
                        "value": usage_id
                    }
                ]
            }
        ])
    }

    /// JSON形式のブロックをSlackBlock形式に変換
    pub fn json_to_slack_blocks(blocks_json: serde_json::Value) -> Vec<SlackBlock> {
        serde_json::from_value(blocks_json.clone()).unwrap_or_else(|_| vec![])
    }
}
