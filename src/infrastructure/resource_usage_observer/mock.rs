use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::ports::resource_usage_observer::{
    ObservationError, ObservationSnapshot, ObservedUsage, ResourceUsageObserver, ServerObservation,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::{Arc, Mutex};

/// テスト/開発用のインメモリ観測実装
///
/// `set_active_usages`で観測結果を差し替えることで、実サーバーの状態変化を模擬できる。
#[derive(Clone, Default)]
pub struct MockResourceUsageObserver {
    snapshot: Arc<Mutex<ObservationSnapshot>>,
}

impl MockResourceUsageObserver {
    /// 新しいモック観測実装を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 観測結果を差し替える
    ///
    /// 利用が観測されたサーバーは、観測できたサーバーとして扱う。
    /// 「観測できたが誰も使っていないサーバー」や「観測できなかったサーバー」を
    /// 模擬したい場合は`set_snapshot`を使う。
    pub fn set_active_usages(&self, usages: Vec<ObservedUsage>) {
        let now = Utc::now();
        let servers = usages
            .iter()
            .filter_map(server_of)
            .map(|server| (server, ServerObservation::Observed { generated_at: now }))
            .collect();
        *self.snapshot.lock().unwrap() = ObservationSnapshot::new(usages, servers);
    }

    /// 観測できたサーバーまで指定して観測結果を差し替える
    pub fn set_snapshot(&self, snapshot: ObservationSnapshot) {
        *self.snapshot.lock().unwrap() = snapshot;
    }
}

/// 観測された利用が乗っているサーバー名（GPU以外のリソースはサーバーを持たない）
fn server_of(usage: &ObservedUsage) -> Option<String> {
    match usage.resource() {
        Resource::Gpu(gpu) => Some(gpu.server().to_string()),
        Resource::Room { .. } => None,
    }
}

#[async_trait]
impl ResourceUsageObserver for MockResourceUsageObserver {
    async fn observe_active_usages(&self) -> Result<ObservationSnapshot, ObservationError> {
        Ok(self.snapshot.lock().unwrap().clone())
    }
}
