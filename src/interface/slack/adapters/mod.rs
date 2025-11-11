//! 他レイヤーとのアダプター
//!
//! Domain層およびApplication層との統合を提供します。
//!
//! ## 責務
//!
//! このモジュールは、Slack固有のデータ（SlackユーザーIDなど）を
//! ドメイン層の概念（EmailAddressなど）に変換する役割を担います。
//! クリーンアーキテクチャにおけるAdapter層に相当し、外部システム（Slack）と
//! 内部ドメインの間の橋渡しを行います。
//!
//! ## モジュール
//!
//! - `user_resolver`: SlackユーザーIDからメールアドレスへの解決

pub mod user_resolver;
