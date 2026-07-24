//! 実サーバー利用状況観測の実装
//!
//! 各GPUサーバーのcronスクリプト（`scripts/gpu_usage_reporter.py`）が共有ファイルシステムへ
//! 書き出すJSONを読み取る`SharedFileResourceUsageObserver`を提供する。
//! SSHポーリングやPrometheus/DCGM Exporter等、他の監視手段は将来必要に応じて追加する。

/// テスト/開発用のインメモリ観測実装
pub mod mock;
/// 共有ファイルシステム経由の観測実装
pub mod shared_file;

pub use mock::MockResourceUsageObserver;
pub use shared_file::SharedFileResourceUsageObserver;
