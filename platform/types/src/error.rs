use std::fmt;

/// Validation failure for a public identifier. Never pass secret material here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    value: String,
    rule: &'static str,
}

impl IdentifierError {
    pub fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_owned(),
            rule,
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identifier {:?}: {}", self.value, self.rule)
    }
}

impl std::error::Error for IdentifierError {}
