//! # Value Objects（値オブジェクト）
//!
//! DDDにおける値オブジェクトは、識別子を持たず、その属性の値によってのみ定義されるオブジェクトです。
//!
//! ## 値オブジェクトの特徴
//!
//! - **不変性**: 一度作成されたら変更できない
//! - **等価性**: すべての属性が等しければ同じオブジェクトとみなされる
//! - **自己検証**: 生成時に不正な値を拒否し、常に有効な状態を保つ
//! - **副作用なし**: メソッドは新しい値オブジェクトを返し、自身を変更しない
pub mod resource;
pub mod time_period;
pub mod usage_id;

pub use resource::{Gpu, Resource};
pub use time_period::TimePeriod;
pub use usage_id::UsageId;
