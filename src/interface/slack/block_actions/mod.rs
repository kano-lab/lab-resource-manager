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

pub mod cancel_button;
pub mod edit_button;
pub mod modal_state_change;
