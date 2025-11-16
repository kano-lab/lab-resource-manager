//! スラッシュコマンドハンドラ
//!
//! Slack Slash Commands イベントの処理を行います。
//!
//! ## 責務
//!
//! Slackのスラッシュコマンド（`/register-calendar`、`/link-user`など）が実行された際の
//! 処理を担当します。各コマンドに対して1つのモジュールが対応します。
//!
//! ## Slack API との対応
//!
//! このモジュールは、Slack APIの「Slash Commands」イベントタイプに対応します。
//! ユーザーがSlack上で `/コマンド名` を入力すると、Slackから`SlackCommandEvent`が
//! 送信され、このモジュールのハンドラが呼び出されます。
//!
//! ## モジュール
//!
//! - `link_user`: `/link-user` - ユーザーとメールアドレスの紐付け（管理者用）
//! - `register_calendar`: `/register-calendar` - メールアドレス登録（モーダルベース）

pub mod link_user;
pub mod register_calendar;
