use veoveo_types::{IdentifierError, ScopeDefinition, ScopeName};

/// Optimization declares no additional domain scopes. Gateway operation policy and
/// current owner, Work Context and label checks still authorize every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationScope {}

impl ScopeDefinition for OptimizationScope {
    fn name(self) -> &'static ScopeName {
        match self {}
    }
}

impl TryFrom<&ScopeName> for OptimizationScope {
    type Error = IdentifierError;
    fn try_from(value: &ScopeName) -> Result<Self, Self::Error> {
        Err(IdentifierError::new(
            value.as_str(),
            "Optimization declares no domain scopes",
        ))
    }
}
