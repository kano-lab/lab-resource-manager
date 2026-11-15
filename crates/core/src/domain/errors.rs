/// ドメイン層のエラーを表すマーカートレイト
pub trait DomainError: std::error::Error + Send + Sync {}
