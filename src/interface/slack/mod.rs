//! Slackインターフェース
//!
//! クリーンアーキテクチャに基づくSlack統合の実装
//!
//! ## アーキテクチャ
//!
//! このモジュールはSlack公式APIの概念とイベントモデルに基づいて構成されています。
//! Slackは「Slash Commands」「View Submissions」という主要な
//! インタラクションタイプを定義しており、これらに対応したディレクトリ構造を採用しています。
//!
//! ### モジュール構成
//!
//! - `app`: 依存性注入を備えたアプリケーションコア
//! - `gateway`: Slackイベントのルーティング（イベント種別に応じたハンドラへの振り分け）
//! - `slash_commands`: スラッシュコマンドハンドラ（`/register-calendar`、`/link-user`）
//! - `block_actions`: ブロックアクションハンドラ（モーダル内ボタンクリックなど）
//! - `view_submissions`: モーダル送信ハンドラ（フォーム送信時の処理）
//! - `utility`: ユーティリティ関数（データ抽出、ユーザーID解決など）
//! - `slack_client`: Slack API クライアント（モーダル操作、メッセージ送信）
//! - `async_execution`: バックグラウンドタスク管理（非同期処理）
//! - `views`: UIコンポーネント定義（モーダル、メッセージのビルダー）
//! - `constants`: アクションID、コールバックIDなどの定数
//!
//! ## Slack APIとの対応
//!
//! | Slack概念 | このモジュールの対応 |
//! |-----------|---------------------|
//! | Slash Commands | `slash_commands/` |
//! | View Submissions | `view_submissions/` |
//! | Modals API | `slack_client/modals.rs` |
//! | Messages API | `slack_client/messages.rs` |
//! | Block Kit | `views/` |

pub mod app;
pub mod async_execution;
pub mod constants;
pub mod gateway;
pub mod slack_client;
pub mod slash_commands;
pub mod utility;
pub mod view_submissions;
pub mod views;

// 主要な型を再エクスポート
pub use app::SlackApp;
