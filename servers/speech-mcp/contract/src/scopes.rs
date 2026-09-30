use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Speech uses gateway operation policy and current source/session authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechScope {}
impl ScopeDefinition for SpeechScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for SpeechScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Speech declares no domain scopes",
        ))
    }
}
