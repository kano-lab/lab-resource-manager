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

pub mod error;
pub mod notifier;
pub mod repositories;
pub mod resource_collection_access;

pub use error::PortError;
pub use notifier::{NotificationError, NotificationEvent, Notifier};
pub use resource_collection_access::{
    ResourceCollectionAccessError, ResourceCollectionAccessService,
};
