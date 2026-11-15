//! ビュー送信ハンドラ
//!
//! Slack View Submission イベントの処理を行います。
//!
//! ## 責務
//!
//! モーダル（ダイアログ）のフォーム送信時の処理を担当します。
//! ユーザーがモーダル内のフォームに入力して送信ボタンを押すと、
//! このモジュールのハンドラが呼び出されます。
//!
//! ## Slack API との対応
//!
//! このモジュールは、Slack APIの「View Submission」イベントタイプに対応します。
//! モーダルには`callback_id`が設定されており、送信時にその値に基づいて
//! 適切なハンドラにルーティングされます。
//!
//! | callback_id | ハンドラ | 処理内容 |
//! |-------------|---------|---------|
//! | `register_email` | `registration` | メールアドレス登録 |
//! | `link_user` | `link_user` | ユーザーリンク（管理者用） |
//! | `reserve_submit` | `reserve` | リソース予約作成 |
//!
//! ## モジュール
//!
//! - `registration`: メールアドレス登録モーダルの送信処理
//! - `link_user`: ユーザーリンクモーダルの送信処理
//! - `reserve`: リソース予約作成モーダルの送信処理

pub mod link_user;
pub mod registration;
pub mod reserve;
pub mod update;
