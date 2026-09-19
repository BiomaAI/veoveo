use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentIdentifierError;
impl fmt::Display for AgentIdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "agent identifier must contain 1–128 lowercase letters, digits, hyphens or underscores",
        )
    }
}
impl Error for AgentIdentifierError {}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, AgentIdentifierError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 128
                    || !value.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                    })
                {
                    return Err(AgentIdentifierError);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = AgentIdentifierError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
identifier!(AgentDefinitionId);
identifier!(AgentModelId);
identifier!(AgentTemplateId);
