//! # IdentityLink Repository Implementations
//!
//! IdentityLinkRepositoryポートの具象実装を提供します。
//!
//! - `json_file`: JSONファイルを使用した永続化実装
pub mod json_file;

pub use json_file::JsonFileIdentityLinkRepository;
