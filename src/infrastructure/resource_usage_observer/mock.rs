use crate::domain::ports::resource_usage_observer::{
    ObservationError, ObservedUsage, ResourceUsageObserver,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// テスト/開発用のインメモリ観測実装
///
/// `set_active_usages`で観測結果を差し替えることで、実サーバーの状態変化を模擬できる。
#[derive(Clone, Default)]
pub struct MockResourceUsageObserver {
    active_usages: Arc<Mutex<Vec<ObservedUsage>>>,
}

impl MockResourceUsageObserver {
    /// 新しいモック観測実装を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 観測結果を差し替える
    pub fn set_active_usages(&self, usages: Vec<ObservedUsage>) {
        *self.active_usages.lock().unwrap() = usages;
    }
}

#[async_trait]
impl ResourceUsageObserver for MockResourceUsageObserver {
    async fn observe_active_usages(&self) -> Result<Vec<ObservedUsage>, ObservationError> {
        Ok(self.active_usages.lock().unwrap().clone())
    }
}
