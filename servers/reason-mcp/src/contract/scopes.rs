use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Reason declares no additional domain scopes. Gateway operation policy and
/// current owner and label checks still authorize every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonScope {}

impl ScopeDefinition for ReasonScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}

impl TryFrom<&ScopeName> for ReasonScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Reason declares no domain scopes",
        ))
    }
}
