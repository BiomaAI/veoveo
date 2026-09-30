use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Computers uses gateway operation policy and domain-owned current authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComputerScope {}
impl ScopeDefinition for ComputerScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for ComputerScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Computers declares no domain scopes",
        ))
    }
}
