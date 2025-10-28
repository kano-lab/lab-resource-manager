//! # Interface Layer
//!
//! クリーンアーキテクチャのInterface層（Adapter層）
//!
//! ## Interface層とは
//!
//! 外部からのリクエストを受け取り、適切なUseCase（Application層）に振り分ける層。
//! CLI、Web API、Slackボットなど、さまざまなインターフェースを提供する。
//!
//! ## 依存のルール
//!
//! Interface層はApplication層とDomain層に依存できる。
//! Infrastructure層には直接依存しない（DIコンテナ経由で注入）。
pub mod slack;
