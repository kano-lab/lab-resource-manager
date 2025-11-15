//! # Use Cases（ユースケース）
//!
//! ## Use Caseとは
//!
//! Use Caseは、**1つのユーザーゴールを達成するためのビジネスフロー**を表現します。
//! エンティティやドメインサービスを組み合わせ（オーケストレート）、アプリケーション固有の処理を実現します。
//!
//! ## Use Caseの責務
//!
//! ### ✅ Use Caseが行うこと
//! - ポート（トレイト）を経由した外部システムとの通信
//! - ドメインエンティティ・サービスの呼び出しと調整
//! - トランザクション境界の定義
//! - アプリケーション固有のビジネスフローの実装
//!
//! ### ❌ Use Caseが行わないこと
//! - ドメインロジック（ビジネスルール）の実装
//!   → Domain層のエンティティ・値オブジェクト・ドメインサービスが担当
//! - 技術的詳細の実装
//!   → Infrastructure層のアダプターが担当
//!
//! ## 設計原則
//!
//! ### 1. Single Responsibility Principle (SRP)
//! 各Use Caseは**1つの変更理由**のみを持つ。
//! - `WatchCalendarChangesUseCase`: カレンダー監視のビジネスフローが変わる
//! - `CreateUsageUseCase`: 使用予定作成のビジネスフローが変わる
//!
//! ### 2. Dependency Inversion Principle (DIP)
//! Use Caseは抽象（ポート）に依存し、具象には依存しない。
//! ```rust,ignore
//! pub struct SomeUseCase<R, N>
//! where
//!     R: ResourceUsageRepository,  // trait（抽象）に依存
//!     N: Notifier,                 // trait（抽象）に依存
//! ```
//!
//! ### 3. Interface Segregation Principle (ISP)
//! Use Caseは必要最小限のポートにのみ依存する。
//!
//! ### 4. Thin Application Layer
//! Application層は薄く保ち、ドメインロジックをDomain層に配置する。

/// リソース使用予定を作成するユースケース
pub mod create_resource_usage;
/// リソース使用予定を削除するユースケース
pub mod delete_resource_usage;
/// IDでリソース使用予定を取得するユースケース
pub mod get_resource_usage_by_id;
/// ユーザーにリソースアクセス権を付与するユースケース
pub mod grant_user_resource_access;
/// 全ての未来のリソース使用予定を取得するユースケース
pub mod list_all_future_resource_usages;
/// ユーザーのリソース使用予定一覧を取得するユースケース
pub mod list_user_resource_usages;
/// 未来のリソース使用変更を監視して通知するユースケース
pub mod notify_future_resource_usage_changes;
/// リソース使用予定を更新するユースケース
pub mod update_resource_usage;

pub use create_resource_usage::CreateResourceUsageUseCase;
pub use delete_resource_usage::DeleteResourceUsageUseCase;
pub use get_resource_usage_by_id::GetResourceUsageByIdUseCase;
pub use grant_user_resource_access::GrantUserResourceAccessUseCase;
pub use list_all_future_resource_usages::ListAllFutureResourceUsagesUseCase;
pub use list_user_resource_usages::ListUserResourceUsagesUseCase;
pub use notify_future_resource_usage_changes::NotifyFutureResourceUsageChangesUseCase;
pub use update_resource_usage::UpdateResourceUsageUseCase;
