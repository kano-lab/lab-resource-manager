//! # Resource Factory
//!
//! DDDのFactory Patternを用いて、複雑なResourceオブジェクトの生成をカプセル化します。
//!
//! ## Factory Pattern
//!
//! Factoryは、複雑な生成ロジックやドメイン知識を持つオブジェクトの生成を担当します。
//! 特に、外部システムの記法（例: `"0-2,5,7-9"`）からドメインモデルへの変換を行います。

/// リソースファクトリの実装
pub mod resource_factory;

pub use resource_factory::{ResourceFactory, ResourceFactoryError};
