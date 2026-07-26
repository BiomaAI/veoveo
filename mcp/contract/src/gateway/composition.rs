use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

pub const GATEWAY_SERVER_FRAGMENT_SCHEMA: &str = "veoveo.io/gateway-server-fragment/v1";
pub const GATEWAY_BINDING_SCHEMA: &str = "veoveo.io/gateway-binding/v1";
pub const GATEWAY_COMPOSITION_PROVENANCE_SCHEMA: &str =
    "veoveo.io/gateway-composition-provenance/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GatewayServerFragmentSchema {
    #[serde(rename = "veoveo.io/gateway-server-fragment/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GatewayBindingSchema {
    #[serde(rename = "veoveo.io/gateway-binding/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GatewayCompositionProvenanceSchema {
    #[serde(rename = "veoveo.io/gateway-composition-provenance/v1")]
    V1,
}

/// Extension-owned server capabilities and installation requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayServerFragment {
    pub schema_version: GatewayServerFragmentSchema,
    pub server: ServerManifest,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_platform_capabilities: BTreeSet<PlatformCapabilityId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_artifact_audiences: BTreeSet<ArtifactAudience>,
    #[serde(default)]
    pub recording_producer_required: bool,
    #[serde(default)]
    pub metadata: Value,
}

/// Installation-owned exposure of one contributed server in one existing profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayProfileServerBinding {
    pub profile: GatewayProfileId,
    pub tools: Exposure<LocalToolName>,
    pub resources: Exposure<ResourceSelector>,
    pub prompts: Exposure<PromptName>,
    pub completions: CompletionExposure,
    pub tasks: TaskExposure,
}

impl GatewayProfileServerBinding {
    fn exposure(&self, server: ServerSlug) -> ProfileServerExposure {
        ProfileServerExposure {
            server,
            tools: self.tools.clone(),
            resources: self.resources.clone(),
            prompts: self.prompts.clone(),
            completions: self.completions.clone(),
            tasks: self.tasks.clone(),
        }
    }
}

/// Installation-owned rules appended to one existing policy version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayPolicyRuleBinding {
    pub policy_version: PolicyVersion,
    pub rules: Vec<PolicyRule>,
}

/// Installation-owned recording registration for one existing ingest resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayRecordingProducerBinding {
    pub resource: ProtectedResourceName,
    pub producer: RecordingProducerRegistration,
}

/// Installation-owned exposure, authorization, and producer policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayBinding {
    pub schema_version: GatewayBindingSchema,
    pub server: ServerSlug,
    pub profiles: Vec<GatewayProfileServerBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_rules: Vec<GatewayPolicyRuleBinding>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_artifact_audiences: BTreeSet<ArtifactAudience>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recording_producers: Vec<GatewayRecordingProducerBinding>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayCompositionRequirements {
    pub platform_capabilities: BTreeSet<PlatformCapabilityId>,
    pub artifact_audiences: BTreeSet<ArtifactAudience>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedGatewayControlPlane {
    pub control_plane: GatewayControlPlane,
    pub requirements: GatewayCompositionRequirements,
    pub contributions: Vec<GatewayCompositionContribution>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayCompositionInputKind {
    BaseControlPlane,
    ServerFragment,
    Binding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayCompositionInput {
    pub kind: GatewayCompositionInputKind,
    pub identity: String,
    pub schema_version: String,
    pub sha256: CompositionDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayCompositionContribution {
    pub server: ServerSlug,
    pub profiles: BTreeSet<GatewayProfileId>,
    pub policies: BTreeSet<PolicyVersion>,
    pub recording_producers: BTreeSet<RecordingProducerId>,
    pub platform_capabilities: BTreeSet<PlatformCapabilityId>,
    pub artifact_audiences: BTreeSet<ArtifactAudience>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayCompositionProvenance {
    pub schema_version: GatewayCompositionProvenanceSchema,
    pub output_sha256: CompositionDigest,
    pub inputs: Vec<GatewayCompositionInput>,
    pub contributions: Vec<GatewayCompositionContribution>,
    pub requirements: GatewayCompositionRequirements,
}

#[derive(Debug)]
pub enum GatewayCompositionError {
    DuplicateFragment(ServerSlug),
    DuplicateBinding(ServerSlug),
    MissingBinding(ServerSlug),
    UnknownBindingServer(ServerSlug),
    UnknownProfile {
        server: ServerSlug,
        profile: GatewayProfileId,
    },
    UnknownPolicy {
        server: ServerSlug,
        policy: PolicyVersion,
    },
    UnknownRecordingResource {
        server: ServerSlug,
        resource: ProtectedResourceName,
    },
    ArtifactAudienceNotAllowed {
        server: ServerSlug,
        audience: ArtifactAudience,
    },
    MissingRecordingProducer(ServerSlug),
    InvalidControlPlane(GatewayControlPlaneError),
}

impl fmt::Display for GatewayCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateFragment(server) => {
                write!(
                    formatter,
                    "duplicate gateway fragment for server `{server}`"
                )
            }
            Self::DuplicateBinding(server) => {
                write!(formatter, "duplicate gateway binding for server `{server}`")
            }
            Self::MissingBinding(server) => {
                write!(
                    formatter,
                    "server fragment `{server}` has no installation binding"
                )
            }
            Self::UnknownBindingServer(server) => {
                write!(
                    formatter,
                    "gateway binding references unknown fragment `{server}`"
                )
            }
            Self::UnknownProfile { server, profile } => write!(
                formatter,
                "binding for server `{server}` references unknown profile `{profile}`"
            ),
            Self::UnknownPolicy { server, policy } => write!(
                formatter,
                "binding for server `{server}` references unknown policy `{policy}`"
            ),
            Self::UnknownRecordingResource { server, resource } => write!(
                formatter,
                "binding for server `{server}` references unknown recording resource `{resource}`"
            ),
            Self::ArtifactAudienceNotAllowed { server, audience } => write!(
                formatter,
                "binding for server `{server}` does not allow required artifact audience `{audience}`"
            ),
            Self::MissingRecordingProducer(server) => write!(
                formatter,
                "server fragment `{server}` requires an installation-owned recording producer"
            ),
            Self::InvalidControlPlane(error) => {
                write!(
                    formatter,
                    "composed gateway control plane is invalid: {error}"
                )
            }
        }
    }
}

impl std::error::Error for GatewayCompositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidControlPlane(error) => Some(error),
            _ => None,
        }
    }
}

impl From<GatewayControlPlaneError> for GatewayCompositionError {
    fn from(error: GatewayControlPlaneError) -> Self {
        Self::InvalidControlPlane(error)
    }
}

/// Deterministically combines capabilities with installation-owned authority.
pub fn compose_gateway_control_plane(
    mut base: GatewayControlPlane,
    fragments: Vec<GatewayServerFragment>,
    bindings: Vec<GatewayBinding>,
) -> Result<ComposedGatewayControlPlane, GatewayCompositionError> {
    base.validate()?;

    let mut fragment_by_server = BTreeMap::new();
    for fragment in fragments {
        let server = fragment.server.slug.clone();
        if fragment_by_server
            .insert(server.clone(), fragment)
            .is_some()
        {
            return Err(GatewayCompositionError::DuplicateFragment(server));
        }
    }
    let mut binding_by_server = BTreeMap::new();
    for binding in bindings {
        let server = binding.server.clone();
        if binding_by_server.insert(server.clone(), binding).is_some() {
            return Err(GatewayCompositionError::DuplicateBinding(server));
        }
    }
    for server in fragment_by_server.keys() {
        if !binding_by_server.contains_key(server) {
            return Err(GatewayCompositionError::MissingBinding(server.clone()));
        }
    }
    for server in binding_by_server.keys() {
        if !fragment_by_server.contains_key(server) {
            return Err(GatewayCompositionError::UnknownBindingServer(
                server.clone(),
            ));
        }
    }

    let profile_indices = base
        .profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| (profile.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let policy_indices = base
        .policies
        .iter()
        .enumerate()
        .map(|(index, policy)| (policy.version.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let recording_indices = base
        .recording_ingest_resources
        .iter()
        .enumerate()
        .map(|(index, resource)| (resource.id.clone(), index))
        .collect::<BTreeMap<_, _>>();

    let mut requirements = GatewayCompositionRequirements {
        platform_capabilities: BTreeSet::new(),
        artifact_audiences: BTreeSet::new(),
    };
    let mut contributions = Vec::new();
    for (server, fragment) in fragment_by_server {
        let binding = binding_by_server
            .remove(&server)
            .expect("binding presence was validated");
        for audience in &fragment.required_artifact_audiences {
            if !binding.allowed_artifact_audiences.contains(audience) {
                return Err(GatewayCompositionError::ArtifactAudienceNotAllowed {
                    server: server.clone(),
                    audience: audience.clone(),
                });
            }
        }
        if fragment.recording_producer_required && binding.recording_producers.is_empty() {
            return Err(GatewayCompositionError::MissingRecordingProducer(
                server.clone(),
            ));
        }

        let mut contribution = GatewayCompositionContribution {
            server: server.clone(),
            profiles: BTreeSet::new(),
            policies: BTreeSet::new(),
            recording_producers: BTreeSet::new(),
            platform_capabilities: fragment.required_platform_capabilities.clone(),
            artifact_audiences: binding.allowed_artifact_audiences.clone(),
        };
        for profile_binding in binding.profiles {
            let Some(index) = profile_indices.get(&profile_binding.profile).copied() else {
                return Err(GatewayCompositionError::UnknownProfile {
                    server: server.clone(),
                    profile: profile_binding.profile,
                });
            };
            contribution
                .profiles
                .insert(profile_binding.profile.clone());
            base.profiles[index]
                .servers
                .push(profile_binding.exposure(server.clone()));
        }
        for policy_binding in binding.policy_rules {
            let Some(index) = policy_indices.get(&policy_binding.policy_version).copied() else {
                return Err(GatewayCompositionError::UnknownPolicy {
                    server: server.clone(),
                    policy: policy_binding.policy_version,
                });
            };
            contribution
                .policies
                .insert(policy_binding.policy_version.clone());
            base.policies[index].rules.extend(policy_binding.rules);
        }
        for recording_binding in binding.recording_producers {
            let Some(index) = recording_indices.get(&recording_binding.resource).copied() else {
                return Err(GatewayCompositionError::UnknownRecordingResource {
                    server: server.clone(),
                    resource: recording_binding.resource,
                });
            };
            contribution
                .recording_producers
                .insert(recording_binding.producer.id.clone());
            base.recording_ingest_resources[index]
                .producers
                .push(recording_binding.producer);
        }
        requirements
            .platform_capabilities
            .extend(fragment.required_platform_capabilities);
        requirements
            .artifact_audiences
            .extend(binding.allowed_artifact_audiences);
        base.servers.push(fragment.server);
        contributions.push(contribution);
    }

    for profile in &mut base.profiles {
        profile
            .servers
            .sort_by(|left, right| left.server.cmp(&right.server));
    }
    for policy in &mut base.policies {
        policy.rules.sort_by(|left, right| left.id.cmp(&right.id));
    }
    for resource in &mut base.recording_ingest_resources {
        resource
            .producers
            .sort_by(|left, right| left.id.cmp(&right.id));
    }
    base.servers
        .sort_by(|left, right| left.slug.cmp(&right.slug));
    contributions.sort_by(|left, right| left.server.cmp(&right.server));
    base.validate()?;

    Ok(ComposedGatewayControlPlane {
        control_plane: base,
        requirements,
        contributions,
    })
}

#[must_use]
pub fn gateway_server_fragment_schema() -> Schema {
    schema_for!(GatewayServerFragment)
}

#[must_use]
pub fn gateway_binding_schema() -> Schema {
    schema_for!(GatewayBinding)
}

#[must_use]
pub fn gateway_composition_provenance_schema() -> Schema {
    schema_for!(GatewayCompositionProvenance)
}
