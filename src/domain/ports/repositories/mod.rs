//! # Repository Ports（リポジトリポート）
//!
//! ## Repositoryパターン
//!
//! Repositoryは**永続化の詳細を隠蔽し、コレクションのような抽象を提供**するパターン。
//!
//! ## 依存性逆転の原則（DIP）
//! ```text
//! Domain層（ポート定義）
//!    ↑
//!    | 依存
//!    |
//! Infrastructure層（実装）
//! ```
//!
//! ## 設計原則
//!
//! ### 1. コレクション指向
//! データベースではなく「コレクション」として扱う。
//!
//! ### 2. ドメイン駆動のクエリ
//! ```rust,ignore
//! // ✅ ドメインの言葉
//! async fn find_slots_overlapping(&self, time_period: &TimePeriod) -> Result<Vec<SlotView>>;
//!
//! // ❌ 技術の言葉
//! async fn find_by_sql(&self, query: &str) -> Result<Vec<SlotView>>;
//! ```
//!
//! ### 3. 集約単位で定義
//! Repositoryは**集約ルート**に対して1つ定義する。
//! 集約内部の値オブジェクトには個別のRepositoryを作らない。
//!
//! 予約と予約枠は別々の集約なので、`ReservationRepository`と`ReservedSlotQuery`を
//! 分けて定義する。同じ永続化層（カレンダー）を使う実装が、両方を実装してよい。

/// リポジトリのエラー型
pub mod errors;
/// IdentityLinkリポジトリポート
pub mod identity_link;
/// McpTokenリポジトリポート
pub mod mcp_token;
/// 予約リポジトリポート
pub mod reservation;
/// 予約枠リポジトリポート
pub mod reserved_slot_query;

pub use errors::RepositoryError;
pub use identity_link::IdentityLinkRepository;
pub use mcp_token::McpTokenRepository;
pub use reservation::ReservationRepository;
pub use reserved_slot_query::ReservedSlotQuery;
