//! # Application Layer
//!
//! このディレクトリはクリーンアーキテクチャのApplication層を表す。
//!
//! ## Application層とは
//!
//! ユースケースを実装し、ドメインロジックを組み合わせてビジネスフローを実現する層。
//! ドメイン層のエンティティやサービスを協調させ、アプリケーション固有の処理を提供する。
//!
//! ## 依存のルール
//!
//! Application層はDomain層にのみ依存する。
//! Infrastructure層への直接の依存は禁止。

/// Application層で発生するエラーの定義
pub mod error;
pub mod usecases;

pub use error::ApplicationError;
