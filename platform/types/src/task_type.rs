//! Open Task operation vocabulary. Each domain owns its declarations.
use std::{borrow::Cow, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::IdentifierError;

/// A Task operation name: 1–128 ASCII bytes, starting with a lowercase letter,
/// followed by lowercase letters, digits, dots, underscores or hyphens.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct TaskTypeName(Cow<'static, str>);

impl TaskTypeName {
    /// Declare a code-owned name. Invalid constant declarations fail to compile.
    /// ```compile_fail
    /// use veoveo_types::TaskTypeName;
    /// const INVALID: TaskTypeName = TaskTypeName::from_static("not a task name");
    /// ```
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid(value), "invalid Task operation name");
        Self(Cow::Borrowed(value))
    }

    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if !valid(&value) {
            return Err(IdentifierError::new(
                &value,
                "expected 1..=128 lowercase ASCII operation-name bytes, starting with a letter; digits, dot, underscore and hyphen are allowed",
            ));
        }
        Ok(Self(Cow::Owned(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Implement on a domain-owned closed enum. Shared code owns no operation variants.
pub trait TaskTypeDefinition: Copy + Eq + 'static {
    /// Every supported operation, with one distinct wire spelling per variant.
    const ALL: &'static [Self];

    fn name(self) -> TaskTypeName;

    fn from_name(name: &TaskTypeName) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == *name)
    }

    /// Admit a protocol operation name into the domain's closed vocabulary.
    fn from_wire_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.name().as_str() == name)
    }
}

const fn valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_lowercase()
            && !byte.is_ascii_digit()
            && !matches!(byte, b'.' | b'_' | b'-')
        {
            return false;
        }
        index += 1;
    }
    true
}

impl fmt::Display for TaskTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for TaskTypeName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for TaskTypeName {
    type Err = IdentifierError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for TaskTypeName {
    type Error = IdentifierError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TaskTypeName> for String {
    fn from(value: TaskTypeName) -> Self {
        value.0.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
    #[vocabulary(task_type)]
    enum ExampleKind {
        #[vocabulary(rename = "example.first")]
        First,
        #[vocabulary(rename = "example_second")]
        Second,
    }

    #[test]
    fn declarations_admit_only_their_complete_vocabulary() {
        assert_eq!(ExampleKind::ALL, &[ExampleKind::First, ExampleKind::Second]);
        for kind in ExampleKind::ALL {
            let name = kind.name();
            assert_eq!(ExampleKind::from_name(&name), Some(*kind));
            assert_eq!(ExampleKind::from_wire_name(name.as_str()), Some(*kind));
        }
        let unknown = TaskTypeName::new("unregistered").unwrap();
        assert_eq!(ExampleKind::from_name(&unknown), None);
        assert_eq!(ExampleKind::from_wire_name("Example.First"), None);
        assert_eq!(ExampleKind::from_wire_name("not a task name"), None);
    }

    #[test]
    fn declarations_and_dynamic_names_share_the_wire_profile() {
        const KIND: TaskTypeName = TaskTypeName::from_static("computer.file_transfer");
        assert_eq!(KIND, "computer.file_transfer".parse().unwrap());
        assert_eq!(
            serde_json::to_string(&KIND).unwrap(),
            "\"computer.file_transfer\""
        );
        assert_eq!(
            serde_json::from_str::<TaskTypeName>("\"computer.file_transfer\"").unwrap(),
            KIND
        );
        for invalid in [
            "",
            "Uppercase",
            "0first",
            "white space",
            "quote'",
            "slash/",
            "é",
            "line\n",
        ] {
            assert!(TaskTypeName::new(invalid).is_err());
            assert!(serde_json::from_value::<TaskTypeName>(serde_json::json!(invalid)).is_err());
        }
        assert!(TaskTypeName::new("a".repeat(128)).is_ok());
        assert!(TaskTypeName::new("a".repeat(129)).is_err());
    }
}
