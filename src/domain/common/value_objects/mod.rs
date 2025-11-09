//! # Shared Value Objects
//!
//! 複数の集約で共有される汎用的なValue Objectsを提供します。
//! これらはShared Kernelの一部として、境界を越えて使用されます。

/// メールアドレスの値オブジェクト
pub mod email_address;
/// 共通値オブジェクトのエラー型
pub mod errors;

pub use email_address::EmailAddress;
pub use errors::EmailAddressError;
