use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Artifact uses gateway operation policy and Artifact service access decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactScope {}
impl ScopeDefinition for ArtifactScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for ArtifactScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Artifact declares no domain scopes",
        ))
    }
}
