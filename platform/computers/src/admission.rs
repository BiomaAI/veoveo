use crate::{Computer, ComputerError, ComputersStore, Result, identity::*, model::computer_record};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, deterministic_tenant_id};
use veoveo_task_runtime::TaskOwner;

/// Installation policy, never a caller-controlled request field. Zero closes admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityPolicy {
    pub per_owner: u32,
    pub per_tenant: u32,
    pub provider: u32,
}

#[derive(Clone, Debug)]
pub struct Reservation {
    pub request_id: crate::api::RequestId,
    pub template_id: crate::api::TemplateId,
    pub template_fingerprint: String,
}

impl Reservation {
    fn validate(&self) -> Result<()> {
        if self.template_fingerprint.len() != 64
            || !self
                .template_fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Serialize, SurrealValue)]
struct Content {
    computer_id: Uuid,
    owner_key: String,
    tenant_key: String,
    // Shared TaskOwner envelope is typed on both sides of this store adapter.
    owner_context: OpenObject,
    provider_instance_id: Uuid,
    template_id: String,
    template_fingerprint: String,
}

impl ComputersStore {
    /// Resolve a public Create retry before selecting the current default template.
    /// The request had no template input; its first accepted selection remains binding.
    pub async fn reserved_for_request(
        &self,
        owner: &TaskOwner,
        request_id: crate::api::RequestId,
    ) -> Result<Option<Computer>> {
        let key = owner_key(owner)?;
        let request = RecordId::new(
            "computer_request",
            digest(&("veoveo.computer.create.v1", &key, request_id))?,
        );
        let mut params = crate::store::owner_query_bindings(owner)?;
        params.extend([
            ("request", request.into_value()),
            ("provider", self.provider_instance_id.as_uuid().into_value()),
        ]);
        let mut response = self
            .query(include_str!("../queries/reservation_read.surql"), params)
            .await?;
        let row: Option<crate::model::ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.map(Computer::try_from).transpose()
    }

    /// An exact retry resolves its original Computer, including after a quota reduction.
    pub async fn reserve(
        &self,
        actor: &crate::ComputerActor,
        input: &Reservation,
    ) -> Result<Computer> {
        actor.check_admission()?;
        let owner = actor.owner();
        input.validate()?;
        can_mutate(owner)?;
        let key = owner_key(owner)?;
        let request_key = digest(&("veoveo.computer.create.v1", &key, input.request_id))?;
        let request = RecordId::new("computer_request", request_key);
        let fingerprint = digest(&(
            self.provider_instance_id,
            &input.template_id,
            &input.template_fingerprint,
        ))?;
        self.platform
            .ensure_identity(
                owner.tenant_key(),
                &owner.principal_key,
                &owner.issuer,
                &owner.subject,
                owner.principal_kind,
            )
            .await
            .map_err(|_| ComputerError::Unavailable)?;
        let id = crate::api::ComputerId::new();
        let computer = computer_record(id);
        let content = Content {
            computer_id: id.as_uuid(),
            owner_key: key.clone(),
            tenant_key: owner.tenant_key().into(),
            owner_context: object(owner)?,
            provider_instance_id: self.provider_instance_id.as_uuid(),
            template_id: input.template_id.to_string(),
            template_fingerprint: input.template_fingerprint.clone(),
        };
        let tenant =
            deterministic_tenant_id(owner.tenant_key()).map_err(|_| ComputerError::InvalidInput)?;

        let params = vec![
            crate::audit::binding(
                actor.accepted(),
                id,
                crate::audit::Transition::accepted(
                    veoveo_audit_contract::ComputerActivity::Create,
                    veoveo_audit_contract::ComputerAuditStage::Reserved,
                ),
            )?,
            ("request", request.clone().into_value()),
            ("fingerprint", fingerprint.into_value()),
            ("computer", computer.into_value()),
            ("content", content.into_value()),
            (
                "owner_usage",
                RecordId::new("computer_usage", format!("owner:{}", quota_key(owner)?))
                    .into_value(),
            ),
            (
                "tenant_usage",
                RecordId::new("computer_usage", format!("tenant:{}", tenant)).into_value(),
            ),
            (
                "provider_usage",
                RecordId::new(
                    "computer_usage",
                    format!("provider:{}", self.provider_instance_id),
                )
                .into_value(),
            ),
            ("capacity", self.capacity_record().into_value()),
        ];
        self.query(include_str!("../queries/reserve.surql"), params)
            .await?;
        self.reserved_for_request(owner, input.request_id)
            .await?
            .ok_or(ComputerError::Unavailable)
    }
}

fn object(value: &impl Serialize) -> Result<OpenObject> {
    let serde_json::Value::Object(fields) =
        serde_json::to_value(value).map_err(|_| ComputerError::InvalidInput)?
    else {
        return Err(ComputerError::InvalidInput);
    };
    Ok(OpenObject::new(fields.into_iter().collect()))
}
