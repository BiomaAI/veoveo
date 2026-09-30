use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Media uses gateway operation policy and current owner/label authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaScope {}
impl ScopeDefinition for MediaScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for MediaScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Media declares no domain scopes",
        ))
    }
}
