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
//! - `CreateReservationUseCase`: 予約作成のビジネスフローが変わる
//!
//! ### 2. Dependency Inversion Principle (DIP)
//! Use Caseは抽象（ポート）に依存し、具象には依存しない。
//! ```rust,ignore
//! pub struct SomeUseCase<R, N>
//! where
//!     R: ReservedSlotQuery,  // trait（抽象）に依存
//!     N: Notifier,                 // trait（抽象）に依存
//! ```
//!
//! ### 3. Interface Segregation Principle (ISP)
//! Use Caseは必要最小限のポートにのみ依存する。
//!
//! ### 4. Thin Application Layer
//! Application層は薄く保ち、ドメインロジックをDomain層に配置する。

/// 事後予約提案の受諾
pub mod accept_reservation_proposal;
/// 予約枠を取り消すユースケース
pub mod cancel_slot;
/// 指定期間における各リソースの空きを調べるユースケース
pub mod check_resource_availability;
/// 単発予約を作るユースケース
pub mod create_reservation;
/// 実利用の監視の稼働状況を答えるユースケース
pub mod describe_monitoring;
/// 使われていない予約枠を検知して知らせるユースケース
pub mod detect_idle_slots;
#[cfg(test)]
mod detect_idle_slots_tests;
/// 進行中の予約枠を今の時点で締めるユースケース
pub mod end_slot_early;
/// IDで予約枠を取得するユースケース
pub mod get_slot;
/// ユーザーにリソースアクセス権を付与するユースケース
pub mod grant_user_resource_access;
/// 指定期間の予約枠を一覧するユースケース
pub mod list_slots;
/// 予約者の予約枠を一覧するユースケース
pub mod list_slots_held_by;
/// 予約の変化を監視して通知するユースケース
pub mod notify_reservation_changes;
/// 実サーバーの利用状況と予約枠を突き合わせるユースケース
pub mod reconcile_observed_usages;
#[cfg(test)]
mod reconcile_observed_usages_tests;
#[cfg(test)]
pub mod test_support;
/// 予約枠の期間・備考を更新するユースケース
pub mod update_slot;

pub use accept_reservation_proposal::AcceptReservationProposalUseCase;
pub use cancel_slot::CancelSlotUseCase;
pub use check_resource_availability::CheckResourceAvailabilityUseCase;
pub use create_reservation::CreateReservationUseCase;
pub use describe_monitoring::DescribeMonitoringUseCase;
pub use detect_idle_slots::{DetectIdleSlotsUseCase, IdleCriteria};
pub use end_slot_early::EndSlotEarlyUseCase;
pub use get_slot::GetSlotUseCase;
pub use grant_user_resource_access::GrantUserResourceAccessUseCase;
pub use list_slots::ListSlotsUseCase;
pub use list_slots_held_by::ListSlotsHeldByUseCase;
pub use notify_reservation_changes::NotifyReservationChangesUseCase;
pub use reconcile_observed_usages::ReconcileObservedUsagesUseCase;
pub use update_slot::UpdateSlotUseCase;
