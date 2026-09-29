//! Lexical admission; DuckDB owns SQL grammar and identifier interpretation.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

macro_rules! request_text {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, DuckDbRequestTextError> {
                let value = value.into();
                if value.trim().is_empty() || value.contains('\0') {
                    return Err(DuckDbRequestTextError);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl FromStr for $name {
            type Err = DuckDbRequestTextError;
            fn from_str(value: &str) -> Result<Self, Self::Err> { Self::new(value) }
        }
        impl TryFrom<String> for $name {
            type Error = DuckDbRequestTextError;
            fn try_from(value: String) -> Result<Self, Self::Error> { Self::new(value) }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self { value.0 }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                // Match Rust's Unicode White_Space profile across schema regex engines.
                // ECMAScript and Python disagree about several characters in `\s`.
                schemars::json_schema!({
                    "type":"string",
                    "allOf":[
                        {"pattern":"[^\\u0009-\\u000D\\u0020\\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000]"},
                        {"pattern":"^[^\\u0000]*$"}
                    ],
                    "description":$description
                })
            }
        }
    };
}
request_text!(
    DuckDbSqlText,
    "Nonblank SQL text without NUL. The engine checks grammar, statement count and execution permissions."
);
request_text!(
    DuckDbTableName,
    "Nonblank table name without NUL, preserved verbatim and quoted as one SQL identifier."
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuckDbRequestTextError;
impl fmt::Display for DuckDbRequestTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected nonblank text without NUL")
    }
}
impl std::error::Error for DuckDbRequestTextError {}
