//! Slackインターフェース
//!
//! クリーンアーキテクチャに基づくSlack統合の実装
//!
//! ## アーキテクチャ
//!
//! このモジュールはSlack公式APIの概念とイベントモデルに基づいて構成されています。
//! Slackは「Slash Commands」「View Submissions」「Block Actions」という3つの主要な
//! インタラクションタイプを定義しており、これらに対応したディレクトリ構造を採用しています。
//!
//! ### モジュール構成
//!
//! - `app`: 依存性注入を備えたアプリケーションコア（Builder パターン）
//! - `gateway`: Slackイベントのルーティング（イベント種別に応じたハンドラへの振り分け）
//! - `slash_commands`: スラッシュコマンドハンドラ（`/reserve`、`/register-calendar`など）
//! - `view_submissions`: モーダル送信ハンドラ（フォーム送信時の処理）
//! - `block_actions`: ブロックアクションハンドラ（ボタンクリック、セレクトメニュー操作）
//! - `adapters`: 他レイヤーとの統合（ユーザーID解決など）
//! - `extractors`: Slackペイロードからのデータ抽出ユーティリティ
//! - `slack_client`: Slack API クライアント（モーダル操作、メッセージ送信）
//! - `async_execution`: バックグラウンドタスク管理（非同期処理）
//! - `views`: UIコンポーネント定義（モーダル、メッセージのビルダー）
//! - `parsers`: Slack特有の入力データのパース（日時、リソースIDなど）
//! - `constants`: アクションID、コールバックIDなどの定数
//!
//! ## Slack APIとの対応
//!
//! | Slack概念 | このモジュールの対応 |
//! |-----------|---------------------|
//! | Slash Commands | `slash_commands/` |
//! | View Submissions | `view_submissions/` |
//! | Block Actions | `block_actions/` |
//! | Modals API | `slack_client/modals.rs` |
//! | Messages API | `slack_client/messages.rs` |
//! | Block Kit | `views/` |

pub mod adapters;
pub mod app;
pub mod async_execution;
pub mod block_actions;
pub mod constants;
pub mod extractors;
pub mod gateway;
pub mod parsers;
pub mod slack_client;
pub mod slash_commands;
pub mod view_submissions;
pub mod views;

// 主要な型を再エクスポート
pub use app::SlackApp;
