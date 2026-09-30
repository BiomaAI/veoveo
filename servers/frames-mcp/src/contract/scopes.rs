use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Frames uses gateway operation policy and current owner/label authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramesScope {}
impl ScopeDefinition for FramesScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for FramesScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Frames declares no domain scopes",
        ))
    }
}
