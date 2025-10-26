#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Gpu {
    server: String,     // "Thalys", "Freccia", "Lyria"
    device_number: u32, // 0, 1, 2, ...
    model: String,      // "A100", "RTX6000", ...
}

impl Gpu {
    pub fn new(server: String, device_number: u32, model: String) -> Self {
        Self {
            server,
            device_number,
            model,
        }
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn device_number(&self) -> u32 {
        self.device_number
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    Gpu(Gpu),              // 個別GPU（例: Thalys-0）
    Room { name: String }, // 部屋
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
