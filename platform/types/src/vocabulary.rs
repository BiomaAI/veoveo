//! Domain-owned closed spellings, with no registry of domain variants.
use crate::IdentifierError;

/// A closed vocabulary that an independently developed owner may implement.
///
/// Derive declarations own one spelling per unit variant. The optional scope and
/// Task profiles apply the existing admission rules without registering domains.
///
/// ```
/// #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
/// enum Fruit { Apple, #[vocabulary(rename = "blood-orange")] BloodOrange }
/// assert_eq!(Fruit::Apple.as_str(), "apple");
/// assert_eq!("blood-orange".parse(), Ok(Fruit::BloodOrange));
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// enum Duplicate { #[vocabulary(rename = "same")] First, #[vocabulary(rename = "same")] Second }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// enum EmptySpelling { #[vocabulary(rename = "")] Value }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum InvalidScope { #[vocabulary(rename = "two scopes")] Read }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum InvalidQuote { #[vocabulary(rename = "read\"write")] Read }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum InvalidSlash { #[vocabulary(rename = r"read\write")] Read }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// #[vocabulary(task_type)]
/// enum InvalidTask { #[vocabulary(rename = "not a task name")] Run }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// #[vocabulary(task_type)]
/// enum NoTasks {}
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// enum NonUnit { Value(u8) }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// enum Generic<T> { Value(T) }
/// ```
/// ```compile_fail
/// #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
/// struct NotAnEnum;
/// ```
pub trait Vocabulary: Copy + Eq + 'static {
    const ALL: &'static [Self];
    const NAME: &'static str;
    const UNKNOWN_RULE: &'static str = "unknown vocabulary value";

    fn as_str(self) -> &'static str;

    fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|item| item.as_str() == value)
    }

    fn parse(value: &str) -> Result<Self, IdentifierError> {
        Self::from_wire(value).ok_or_else(|| IdentifierError::new(value, Self::UNKNOWN_RULE))
    }
}

/// Apply the code-owned OAuth token profile; installation scopes have their own profile.
pub const fn is_scope_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if !matches!(bytes[index], 0x21 | 0x23..=0x5b | 0x5d..=0x7e) {
            return false;
        }
        index += 1;
    }
    true
}

/// Build the existing string-enum schema profile used by domain scope declarations.
pub fn scope_vocabulary_schema<T: Vocabulary>() -> schemars::Schema {
    if T::ALL.is_empty() {
        false.into()
    } else {
        schemars::json_schema!({
            "type": "string",
            "enum": T::ALL.iter().map(|value| value.as_str()).collect::<Vec<_>>()
        })
    }
}
