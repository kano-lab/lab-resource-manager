//! # ResourceUsage Repository Implementations
//!
//! ResourceUsageRepositoryポートの具象実装を提供します。
//!
//! - `google_calendar`: Google Calendar APIを使用した実装
//! - `mock`: テスト用のインメモリ実装
//! - `id_mapper`: Domain IDと外部システムのEvent IDのマッピング

/// Google Calendar APIを使用したResourceUsageリポジトリ実装
pub mod google_calendar;
/// Domain IDと外部システムのEvent IDのマッピング
pub mod id_mapper;
/// テスト用のモックResourceUsageリポジトリ実装
pub mod mock;
