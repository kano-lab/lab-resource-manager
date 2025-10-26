use super::user_id::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct User {
    id: UserId,
    name: String,
}

// ? userはValueObjectなのか？
impl User {
    pub fn new(id: UserId, name: String) -> Self {
        Self { id, name }
    }

    pub fn id(&self) -> &UserId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
