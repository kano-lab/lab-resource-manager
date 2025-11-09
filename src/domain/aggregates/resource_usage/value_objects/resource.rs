/// GPU仕様
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Gpu {
    server: String,     // "Thalys", "Freccia", "Lyria"
    device_number: u32, // 0, 1, 2, ...
    model: String,      // "A100", "RTX6000", ...
}

impl Gpu {
    /// 新しいGPU仕様を作成
    ///
    /// # Arguments
    /// * `server` - サーバー名
    /// * `device_number` - デバイス番号
    /// * `model` - GPUモデル名
    pub fn new(server: String, device_number: u32, model: String) -> Self {
        Self {
            server,
            device_number,
            model,
        }
    }

    /// サーバー名を取得
    pub fn server(&self) -> &str {
        &self.server
    }

    /// デバイス番号を取得
    pub fn device_number(&self) -> u32 {
        self.device_number
    }

    /// GPUモデル名を取得
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// リソースの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    /// GPU
    Gpu(Gpu),
    /// 部屋
    Room {
        /// 部屋名
        name: String,
    },
}

impl Resource {
    /// この資源が他の資源と競合するか（同じ資源を指すか）
    pub fn conflicts_with(&self, other: &Resource) -> bool {
        match (self, other) {
            (Resource::Gpu(id1), Resource::Gpu(id2)) => id1 == id2,
            (Resource::Room { name: name1 }, Resource::Room { name: name2 }) => name1 == name2,
            _ => false,
        }
    }
}
