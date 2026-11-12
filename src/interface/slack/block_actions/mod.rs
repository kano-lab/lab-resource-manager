//! ブロックアクションハンドラ
//!
//! Slack Block Actions イベントの処理を行います。
//!
//! ## 責務
//!
//! ボタンクリックやセレクトメニューの選択などのインタラクションを処理します。
//! モーダル内のアクションとメッセージ内のアクションの両方に対応します。
//!
//! ## モジュール
//!
//! - `modal_state_change`: モーダル状態変更（リソースタイプ、サーバー選択）
//! - `cancel_button`: 予約キャンセルボタンハンドラ
//! - `edit_button`: 予約編集ボタンハンドラ
//! - `accept_proposal_button`: 事後予約提案の受諾ボタンハンドラ
//! - `end_early_button`: 予約を今の時点で終了するボタンハンドラ
//! - `idle_slot_buttons`: 未使用予約のお知らせDMのボタンハンドラ
//! - `free_buttons`: 空き状況メッセージの日切り替え・予約ボタンハンドラ

pub mod accept_proposal_button;
pub mod cancel_button;
pub mod edit_button;
pub mod end_early_button;
pub mod free_buttons;
pub mod idle_reserved_slot_buttons;
pub mod modal_state_change;
