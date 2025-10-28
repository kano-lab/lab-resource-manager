use serde::{Deserialize, Serialize};

/// Slackユーザーを識別するID
///
/// IdentityLink集約に固有のValue Object
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlackUserId(String);

impl SlackUserId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
