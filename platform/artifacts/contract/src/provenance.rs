use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{DelegationId, InvocationProvenance, PolicyVersion, PrincipalId};

/// Immutable explanation of how an artifact came into being.
///
/// The invocation enum carries the identities required by its mode. Its Artifact
/// wire representation uses the existing flat `invocationMode`, `initiator`, and
/// `delegationId` fields. Constructing this value does not establish authority.
///
/// ```compile_fail
/// use veoveo_artifact_contract::ArtifactProvenance;
/// use veoveo_types::{InvocationProvenance, PolicyVersion, PrincipalId};
/// let producer = PrincipalId::parse("issuer#worker").unwrap();
/// ArtifactProvenance::new(
///     producer.clone(),
///     InvocationProvenance::Delegated { initiator: producer },
///     PolicyVersion::parse("v1").unwrap(),
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(from = "ArtifactProvenanceWire", into = "ArtifactProvenanceWire")]
#[schemars(description = "Immutable explanation of how an artifact came into being.")]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProvenance {
    pub producer: PrincipalId,
    pub invocation: InvocationProvenance,
    pub policy_revision: PolicyVersion,
}

impl ArtifactProvenance {
    pub fn new(
        producer: PrincipalId,
        invocation: InvocationProvenance,
        policy_revision: PolicyVersion,
    ) -> Self {
        Self {
            producer,
            invocation,
            policy_revision,
        }
    }
}

// This private wire adapter preserves Artifact's published field names while the
// foundation owns the in-memory invocation model. An optional uninhabited type
// admits only omission/null for fields that contradict the selected mode.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
enum NoIdentity {}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "invocationMode",
    deny_unknown_fields
)]
enum ArtifactProvenanceWire {
    Direct {
        producer: PrincipalId,
        initiator: PrincipalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delegation_id: Option<NoIdentity>,
        policy_revision: PolicyVersion,
    },
    Delegated {
        producer: PrincipalId,
        initiator: PrincipalId,
        delegation_id: DelegationId,
        policy_revision: PolicyVersion,
    },
    Automated {
        producer: PrincipalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        initiator: Option<NoIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delegation_id: Option<NoIdentity>,
        policy_revision: PolicyVersion,
    },
}

impl From<ArtifactProvenanceWire> for ArtifactProvenance {
    fn from(wire: ArtifactProvenanceWire) -> Self {
        match wire {
            ArtifactProvenanceWire::Direct {
                producer,
                initiator,
                policy_revision,
                ..
            } => Self::new(
                producer,
                InvocationProvenance::Direct { initiator },
                policy_revision,
            ),
            ArtifactProvenanceWire::Delegated {
                producer,
                initiator,
                delegation_id,
                policy_revision,
            } => Self::new(
                producer,
                InvocationProvenance::Delegated {
                    initiator,
                    delegation_id,
                },
                policy_revision,
            ),
            ArtifactProvenanceWire::Automated {
                producer,
                policy_revision,
                ..
            } => Self::new(producer, InvocationProvenance::Automated, policy_revision),
        }
    }
}

impl From<ArtifactProvenance> for ArtifactProvenanceWire {
    fn from(value: ArtifactProvenance) -> Self {
        let ArtifactProvenance {
            producer,
            invocation,
            policy_revision,
        } = value;
        match invocation {
            InvocationProvenance::Direct { initiator } => Self::Direct {
                producer,
                initiator,
                delegation_id: None,
                policy_revision,
            },
            InvocationProvenance::Delegated {
                initiator,
                delegation_id,
            } => Self::Delegated {
                producer,
                initiator,
                delegation_id,
                policy_revision,
            },
            InvocationProvenance::Automated => Self::Automated {
                producer,
                initiator: None,
                delegation_id: None,
                policy_revision,
            },
        }
    }
}
