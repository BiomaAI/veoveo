use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Stream declares no additional domain scopes. Gateway operation policy and
/// current owner and label checks still authorize every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamScope {}

impl ScopeDefinition for StreamScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}

impl TryFrom<&ScopeName> for StreamScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Stream declares no domain scopes",
        ))
    }
}
