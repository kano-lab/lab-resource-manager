//! # Infrastructure Layer
//!
//! このディレクトリはクリーンアーキテクチャのInfrastructure層を表す。
//!
//! ## Infrastructure層とは
//!
//! 外部システムやフレームワークとの接続を担当し、データベース、API、通知サービスなどの具体的な実装を提供する層。
//! ドメイン層で定義されたポート（トレイト）を実装し、技術的詳細をカプセル化する。
//!
//! ## 依存のルール
//!
//! Infrastructure層はDomain層とApplication層に依存できる。
//! 外部サービス（GoogleカレンダーAPI、Slack等）との統合を担当する。
pub mod config;
pub mod notifier;
pub mod repositories;
