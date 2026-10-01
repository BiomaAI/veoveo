use super::*;
use veoveo_mcp_knowledge_extension::{AccessDescriptor, ReadPolicy};
use veoveo_types::{AccessSubject, PrincipalId, WorkContextId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeProvenanceRecord {
    tenant_key: String,
    owner_key: String,
    work_context: String,
}

impl TimeProvenanceRecord {
    pub(super) fn new(identity: &PlatformIdentity, work_context: &WorkContextId) -> Self {
        Self {
            tenant_key: identity.tenant_key.clone(),
            owner_key: identity.principal_key.clone(),
            work_context: work_context.to_string(),
        }
    }

    pub(crate) fn access(
        &self,
        tenant: &RecordId,
        owner: &RecordId,
        policy: ReadPolicy,
    ) -> anyhow::Result<AccessDescriptor> {
        let invalid = || anyhow::anyhow!("invalid stored Time provenance");
        anyhow::ensure!(
            &veoveo_platform_store::deterministic_tenant_id(&self.tenant_key)?.record_id()
                == tenant
                && &veoveo_platform_store::deterministic_principal_id(
                    &self.tenant_key,
                    &self.owner_key
                )?
                .record_id()
                    == owner,
            "stored Time provenance disagrees with ownership"
        );
        Ok(AccessDescriptor {
            tenant: self.tenant_key.parse().map_err(|_| invalid())?,
            work_context: self.work_context.parse().map_err(|_| invalid())?,
            read_policy: policy,
            owner: AccessSubject::Principal(
                PrincipalId::new(&self.owner_key).map_err(|_| invalid())?,
            ),
            grants: vec![],
            data_labels: vec![],
        })
    }
}
