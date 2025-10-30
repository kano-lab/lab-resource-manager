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
//! async fn find_overlapping(&self, time_period: &TimePeriod) -> Result<Vec<ResourceUsage>>;
//!
//! // ❌ 技術の言葉
//! async fn find_by_sql(&self, query: &str) -> Result<Vec<ResourceUsage>>;
//! ```
//!
//! ### 3. 集約単位で定義
//! Repositoryは**集約ルート**に対して1つ定義する。
//! 集約内部の値オブジェクトには個別のRepositoryを作らない。
pub mod errors;
pub mod identity_link;
pub mod resource_usage;

pub use errors::RepositoryError;
pub use identity_link::IdentityLinkRepository;
pub use resource_usage::ResourceUsageRepository;
