//! # Common Domain Components
//!
//! 複数の集約や境界づけられたコンテキストで共有されるドメイン概念を提供します。
//! これらはプロジェクト全体で共通の言語（Ubiquitous Language）を形成します。
pub mod value_objects;

pub use value_objects::EmailAddress;
