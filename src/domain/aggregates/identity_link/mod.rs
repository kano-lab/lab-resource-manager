//! # IdentityLink集約
//!
//! メールアドレスとSlackユーザーの紐付け情報を管理する集約です。
//!
//! ## 集約ルート
//!
//! `IdentityLink`エンティティが集約ルートとして機能し、紐付け情報全体の整合性を保証します。
pub mod entity;
pub mod errors;
pub mod value_objects;

pub use entity::IdentityLink;
pub use errors::IdentityLinkError;
pub use value_objects::{EmailAddress, SlackUserId};
