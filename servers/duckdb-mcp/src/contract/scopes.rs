use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// DuckDB declares no additional domain scopes. Gateway operation policy and
/// current owner and label checks still authorize every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDbScope {}

impl ScopeDefinition for DuckDbScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}

impl TryFrom<&ScopeName> for DuckDbScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "DuckDB declares no domain scopes",
        ))
    }
}
