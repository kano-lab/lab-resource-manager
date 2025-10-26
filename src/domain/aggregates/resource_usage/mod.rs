//! # ResourceUsage集約
//!
//! 研究室の資源（GPU、部屋）の使用予定を管理する集約です。
//!
//! ## 集約ルート
//!
//! `ResourceUsage`エンティティが集約ルートとして機能し、使用予定全体の整合性を保証します。
pub mod entity;
pub mod errors;
pub mod service;
pub mod value_objects;
