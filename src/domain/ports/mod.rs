//! # Ports（ポート）
//!
//! ヘキサゴナルアーキテクチャにおけるポートは、アプリケーションの境界を定義するインターフェースです。
//!
//! ## ポートとは
//!
//! ポートは、ドメイン層が外部世界とやり取りするための抽象的な契約（トレイト）を定義します。
//! 具体的な実装（アダプター）はInfrastructure層で提供され、ドメイン層はこれらの抽象に依存します。
//!
//! ## 依存性逆転の原則（DIP）
//!
//! ポートをDomain層に配置することで、依存の方向を逆転させます：
//! ```text
//! Domain層（ポート定義）
//!    ↑
//!    | 依存
//!    |
//! Infrastructure層（アダプター実装）
//! ```

/// ポート共通のエラー定義
pub mod error;
/// 使われていない予約の通知ポート
pub mod idle_reservation_notifier;
/// 通知サービスポート
pub mod notifier;
/// リポジトリポート
pub mod repositories;
/// 未予約利用の事後予約提案ポート
pub mod reservation_proposal;
/// リソースコレクションアクセスサービスポート
pub mod resource_collection_access;
/// 実サーバー利用状況の観測ポート
pub mod resource_usage_observer;
/// 無断使用検知の通知ポート
pub mod unauthorized_usage_notifier;

pub use error::PortError;
pub use idle_reservation_notifier::{IdleEvidence, IdleReservation, IdleReservationNotifier};
pub use notifier::{NotificationError, NotificationEvent, Notifier};
pub use reservation_proposal::{ReservationProposal, ReservationProposalNotifier};
pub use resource_collection_access::{
    ResourceCollectionAccessError, ResourceCollectionAccessService,
};
pub use resource_usage_observer::{
    GpuActivity, ObservationError, ObservationSnapshot, ObservedUsage, ResourceUsageObserver,
    ServerObservation,
};
pub use unauthorized_usage_notifier::UnauthorizedUsageNotifier;
