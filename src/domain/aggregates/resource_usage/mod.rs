//! # ResourceUsage集約
//!
//! 研究室の資源（GPU、部屋）の使用予定を管理する集約です。
//!
//! ## 集約ルート
//!
//! `ResourceUsage`エンティティが集約ルートとして機能し、使用予定全体の整合性を保証します。

/// ResourceUsage集約のエンティティ定義
pub mod entity;
/// ResourceUsage集約のエラー型
pub mod errors;
/// ResourceUsage集約のファクトリ
pub mod factory;
/// ResourceUsage集約のドメインサービス
pub mod service;
/// ResourceUsage集約の値オブジェクト
pub mod value_objects;
