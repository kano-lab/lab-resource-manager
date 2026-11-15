use std::fmt;

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

impl fmt::Display for Gpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} GPU#{} ({})",
            self.server, self.device_number, self.model
        )
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
    ///
    /// GPUは、サーバーとデバイス番号が同じなら同じGPUである。型番は説明のための
    /// 情報で、書き方が違っても（設定を書き換えた前後など）同じGPUを指す。
    pub fn conflicts_with(&self, other: &Resource) -> bool {
        match (self, other) {
            (Resource::Gpu(gpu1), Resource::Gpu(gpu2)) => {
                gpu1.server() == gpu2.server() && gpu1.device_number() == gpu2.device_number()
            }
            (Resource::Room { name: name1 }, Resource::Room { name: name2 }) => name1 == name2,
            _ => false,
        }
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resource::Gpu(gpu) => write!(f, "{}", gpu),
            Resource::Room { name } => write!(f, "部屋: {}", name),
        }
    }
}

#[cfg(test)]
mod conflict_tests {
    use super::*;

    fn gpu(server: &str, device_number: u32, model: &str) -> Resource {
        Resource::Gpu(Gpu::new(
            server.to_string(),
            device_number,
            model.to_string(),
        ))
    }

    #[test]
    fn the_same_gpu_conflicts_however_its_model_is_written() {
        assert!(gpu("Thalys", 0, "A100").conflicts_with(&gpu("Thalys", 0, "A100 80GB PCIe")));
    }

    #[test]
    fn another_device_or_server_does_not_conflict() {
        assert!(!gpu("Thalys", 0, "A100").conflicts_with(&gpu("Thalys", 1, "A100")));
        assert!(!gpu("Thalys", 0, "A100").conflicts_with(&gpu("Freccia", 0, "A100")));
    }

    #[test]
    fn rooms_conflict_by_name_and_never_with_a_gpu() {
        let room = |name: &str| Resource::Room {
            name: name.to_string(),
        };

        assert!(room("会議室A").conflicts_with(&room("会議室A")));
        assert!(!room("会議室A").conflicts_with(&room("会議室B")));
        assert!(!room("Thalys").conflicts_with(&gpu("Thalys", 0, "A100")));
    }
}
