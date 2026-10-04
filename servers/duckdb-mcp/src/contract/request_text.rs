//! Lexical admission; DuckDB owns SQL grammar and identifier interpretation.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[doc = "Nonblank SQL text without NUL. The engine checks grammar, statement count and execution permissions."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbSqlText(String);
impl DuckDbSqlText {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbRequestTextError> {
        let value = value.into();
        check_request_text(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for DuckDbSqlText {
    type Err = DuckDbRequestTextError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl TryFrom<String> for DuckDbSqlText {
    type Error = DuckDbRequestTextError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<DuckDbSqlText> for String {
    fn from(value: DuckDbSqlText) -> Self {
        value.0
    }
}
impl fmt::Display for DuckDbSqlText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl JsonSchema for DuckDbSqlText {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbSqlText".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        request_text_schema(
            "Nonblank SQL text without NUL. The engine checks grammar, statement count and execution permissions.",
        )
    }
}

#[doc = "Nonblank table name without NUL, preserved verbatim and quoted as one SQL identifier."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbTableName(String);
impl DuckDbTableName {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbRequestTextError> {
        let value = value.into();
        check_request_text(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for DuckDbTableName {
    type Err = DuckDbRequestTextError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl TryFrom<String> for DuckDbTableName {
    type Error = DuckDbRequestTextError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<DuckDbTableName> for String {
    fn from(value: DuckDbTableName) -> Self {
        value.0
    }
}
impl fmt::Display for DuckDbTableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl JsonSchema for DuckDbTableName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbTableName".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        request_text_schema(
            "Nonblank table name without NUL, preserved verbatim and quoted as one SQL identifier.",
        )
    }
}

#[doc = "Nonblank column name without NUL, preserved verbatim and quoted as one SQL identifier."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbColumnName(String);
impl DuckDbColumnName {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbRequestTextError> {
        let value = value.into();
        check_request_text(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for DuckDbColumnName {
    type Err = DuckDbRequestTextError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl TryFrom<String> for DuckDbColumnName {
    type Error = DuckDbRequestTextError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<DuckDbColumnName> for String {
    fn from(value: DuckDbColumnName) -> Self {
        value.0
    }
}
impl fmt::Display for DuckDbColumnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl JsonSchema for DuckDbColumnName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbColumnName".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        request_text_schema(
            "Nonblank column name without NUL, preserved verbatim and quoted as one SQL identifier.",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected nonblank text without NUL")]
pub struct DuckDbRequestTextError;

fn check_request_text(value: &str) -> Result<(), DuckDbRequestTextError> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(DuckDbRequestTextError);
    }
    Ok(())
}
fn request_text_schema(description: &str) -> schemars::Schema {
    // Match Rust's Unicode White_Space profile across schema regex engines.
    // ECMAScript and Python disagree about several characters in `\s`.
    schemars::json_schema!({
        "type":"string",
        "allOf":[
            {"pattern":"[^\\u0009-\\u000D\\u0020\\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000]"},
            {"pattern":"^[^\\u0000]*$"}
        ],
        "description":description
    })
}

#[cfg(test)]
mod lexical_tests {
    use super::*;
    #[test]
    fn text_admission_preserves_verbatim_unicode_and_controls() {
        for value in [" SELECT 1 ", "a\nb", "a\u{0001}b", "x\u{2000}y"] {
            assert_eq!(DuckDbSqlText::new(value).unwrap().as_str(), value);
            assert_eq!(DuckDbTableName::new(value).unwrap().as_str(), value);
            assert_eq!(DuckDbColumnName::new(value).unwrap().as_str(), value);
        }
        for invalid in ["", " ", "\u{0085}\u{00A0}\u{2000}\u{3000}", "a\0b"] {
            assert!(DuckDbSqlText::new(invalid).is_err());
            assert!(DuckDbTableName::new(invalid).is_err());
            assert!(DuckDbColumnName::new(invalid).is_err());
            assert!(serde_json::from_value::<DuckDbSqlText>(serde_json::json!(invalid)).is_err());
        }
        assert_eq!(
            DuckDbRequestTextError.to_string(),
            "expected nonblank text without NUL"
        );
        assert!(std::error::Error::source(&DuckDbRequestTextError).is_none());
    }
}
