#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsageId(String);

impl UsageId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
