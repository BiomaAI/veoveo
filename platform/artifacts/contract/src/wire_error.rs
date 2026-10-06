//! Input-safe admission failure independent of transport and verified policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
#[error("invalid request: {0}")]
pub struct ArtifactWireError(&'static str);
impl ArtifactWireError {
    pub(crate) fn invalid(detail: &'static str) -> Self {
        Self(detail)
    }
    /// Bare safe detail for the adapter's InvalidRequest, avoiding a repeated prefix.
    pub fn detail(self) -> &'static str {
        self.0
    }
}
