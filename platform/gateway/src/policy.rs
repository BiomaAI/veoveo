//! Gateway composition over the shared current-policy evaluator.
use crate::GatewayCatalog;
use veoveo_mcp_contract::{
    DataLabelDefinition, DataLabelId, GatewayProfile, GatewayProfileId, PolicyDecision, PolicySet,
    PolicyVersion, ServerManifest, ServerSlug, TenantDefinition, TenantId,
};
pub(crate) use veoveo_policy::resource_scheme_from_uri as resource_scheme;
pub use veoveo_policy::{
    PolicyRequest, RecordingIngestPolicyDecision, RecordingIngestPolicyRequest, exposure_contains,
    mcp_method_name, resource_scheme_from_uri,
};

impl veoveo_policy::PolicyCatalogView for GatewayCatalog {
    fn profile(&self, id: &GatewayProfileId) -> Option<&GatewayProfile> {
        self.profile(id)
    }
    fn policy(&self, id: &PolicyVersion) -> Option<&PolicySet> {
        self.policy(id)
    }
    fn data_label(&self, id: &DataLabelId) -> Option<&DataLabelDefinition> {
        self.data_label(id)
    }
    fn tenant(&self, id: &TenantId) -> Option<&TenantDefinition> {
        self.tenant(id)
    }
    fn server(&self, id: &ServerSlug) -> Option<&ServerManifest> {
        self.server(id)
    }
}
impl GatewayCatalog {
    pub fn decide(&self, request: PolicyRequest<'_>) -> PolicyDecision {
        veoveo_policy::decide(self, request)
    }
    pub fn decide_recording_ingest(
        &self,
        request: RecordingIngestPolicyRequest<'_>,
    ) -> RecordingIngestPolicyDecision {
        veoveo_policy::decide_recording_ingest(self, request)
    }
}
