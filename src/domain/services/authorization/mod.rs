//! 認可サービス
//!
//! ドメインオブジェクトに対する操作の認可を管理する。
//!
//! # 概要
//!
//! 認可ポリシーは、ユーザーが特定のリソースに対して操作を実行する権限を持っているかを判断します。
//! 各ドメインオブジェクトに対して専用の認可ポリシーを実装できます。
//!
//! # モジュール
//!
//! - `policy` - 認可ポリシーの基本トレイトとエラー型
//! - `resource_usage_policy` - リソース使用予定の認可ポリシー実装

pub mod policy;
pub mod resource_usage_policy;

pub use policy::{AuthorizationError, AuthorizationPolicy};
pub use resource_usage_policy::ResourceUsageAuthorizationPolicy;
