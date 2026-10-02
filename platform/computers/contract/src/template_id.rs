//! Installation-owned template names, independent of provider fingerprints.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TemplateId(String);

impl TemplateId {
    pub fn parse(value: impl Into<String>) -> Result<Self, TemplateIdError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.as_bytes()[0].is_ascii_alphanumeric()
            || !value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(TemplateIdError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for TemplateId {
    type Err = TemplateIdError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for TemplateId {
    type Error = TemplateIdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<TemplateId> for String {
    fn from(value: TemplateId) -> Self {
        value.0
    }
}
impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl JsonSchema for TemplateId {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("TemplateId")
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,63}$",
            "minLength": 1, "maxLength": 64
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateIdError;
impl fmt::Display for TemplateIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a Computer template name of 1–64 lowercase ASCII letters, digits or hyphens, beginning with a letter or digit")
    }
}
impl std::error::Error for TemplateIdError {}
