use crate::domain::errors::DomainError;

/// ポート層のエラー基底trait
/// 外部システムとの境界で発生するエラー
pub trait PortError: DomainError {}
