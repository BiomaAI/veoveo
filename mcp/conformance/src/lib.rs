//! Domain-neutral certification for a running Veoveo hosted MCP server.

mod profile;
mod report;
mod runner;

pub use profile::{
    HOSTED_MCP_CONTRACT_REVISION, HOSTED_SERVER_PROFILE_SCHEMA, HostedServerConformanceProfile,
    HostedServerProfileSchema, HttpBoundaryProfile, SurfaceExpectation, SurfaceProfile,
};
pub use report::{
    CONFORMANCE_REPORT_SCHEMA, CheckResult, CheckStatus, ConformanceReport,
    ConformanceReportSchema, ObservedImplementation,
};
pub use runner::run_hosted_server_conformance;

#[must_use]
pub fn hosted_server_conformance_profile_schema() -> schemars::Schema {
    schemars::schema_for!(HostedServerConformanceProfile)
}

#[must_use]
pub fn conformance_report_schema() -> schemars::Schema {
    schemars::schema_for!(ConformanceReport)
}

/// Runtime credentials supplied outside the serializable conformance profile.
#[derive(Clone, Default)]
pub struct ConformanceCredentials {
    bearer_token: Option<String>,
}

impl ConformanceCredentials {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            bearer_token: Some(token.into()),
        }
    }

    pub(crate) fn bearer_token(&self) -> Option<&str> {
        self.bearer_token.as_deref()
    }
}
