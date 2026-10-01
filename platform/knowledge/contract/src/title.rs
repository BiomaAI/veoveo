use crate::KnowledgeError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MemberTitle(String);
impl MemberTitle {
    pub fn new(value: impl Into<String>) -> Result<Self, KnowledgeError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.chars().count() > 256
            || value.chars().any(char::is_control)
        {
            return Err(KnowledgeError(
                "member title requires 1..=256 printable characters",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MemberTitle {
    type Error = KnowledgeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<MemberTitle> for String {
    fn from(value: MemberTitle) -> Self {
        value.0
    }
}
