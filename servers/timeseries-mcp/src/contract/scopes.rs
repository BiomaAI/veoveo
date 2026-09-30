use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Timeseries uses gateway operation policy and current owner/label authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeseriesScope {}
impl ScopeDefinition for TimeseriesScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}
impl TryFrom<&ScopeName> for TimeseriesScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Timeseries declares no domain scopes",
        ))
    }
}
