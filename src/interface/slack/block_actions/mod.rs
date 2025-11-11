//! ブロックアクションハンドラ
//!
//! Slack Block Actions イベントの処理を行います。
//!
//! ## 責務
//!
//! ボタンクリック、セレクトメニュー操作などのインタラクティブ要素の
//! アクションを処理します。Block Actionsは2つの文脈で発生します：
//!
//! 1. **メッセージ内のボタン**: 予約通知メッセージの「編集」「キャンセル」ボタン
//! 2. **モーダル内のインタラクション**: リソースタイプ変更、サーバー選択時の動的更新
//!
//! ## Slack API との対応
//!
//! このモジュールは、Slack APIの「Block Actions」イベントタイプに対応します。
//! Block Kit UIで定義されたインタラクティブ要素に`action_id`が設定されており、
//! その値に基づいて適切なハンドラにルーティングされます。
//!
//! | action_id | ハンドラ | コンテキスト | 処理内容 |
//! |-----------|---------|-------------|---------|
//! | `cancel_reservation` | `cancel_button` | メッセージ | 予約のキャンセル |
//! | `edit_reservation` | `edit_button` | メッセージ | 予約編集モーダルを開く |
//! | `reserve_resource_type` | `modal_state_change` | モーダル | リソースタイプ変更時のモーダル更新 |
//! | `reserve_server_select` | `modal_state_change` | モーダル | サーバー選択時のモーダル更新 |
//!
//! ## モジュール
//!
//! - `cancel_button`: 予約キャンセルボタンの処理
//! - `edit_button`: 予約編集ボタンの処理
//! - `modal_state_change`: モーダル状態変更時の動的更新

pub mod cancel_button;
pub mod edit_button;
pub mod modal_state_change;
