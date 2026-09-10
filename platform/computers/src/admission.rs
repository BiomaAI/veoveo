use crate::{Computer, ComputerError, ComputersStore, Result, identity::*, model::computer_record};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, OutboxDraft, deterministic_tenant_id};
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
    pub request_id: Uuid,
    pub template_id: String,
    pub template_fingerprint: String,
}

impl Reservation {
    fn validate(&self) -> Result<()> {
        if self.request_id.is_nil()
            || self.template_id.is_empty()
            || self.template_id.len() > 64
            || !self
                .template_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !self.template_id.as_bytes()[0].is_ascii_alphanumeric()
            || self.template_fingerprint.len() != 64
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
    /// An exact retry resolves its original Computer, including after a quota reduction.
    pub async fn reserve(&self, owner: &TaskOwner, input: &Reservation) -> Result<Computer> {
        input.validate()?;
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
        let id = Uuid::now_v7();
        let computer = computer_record(id);
        let content = Content {
            computer_id: id,
            owner_key: key.clone(),
            tenant_key: owner.tenant_key().into(),
            owner_context: object(owner)?,
            provider_instance_id: self.provider_instance_id,
            template_id: input.template_id.clone(),
            template_fingerprint: input.template_fingerprint.clone(),
        };
        let tenant =
            deterministic_tenant_id(owner.tenant_key()).map_err(|_| ComputerError::InvalidInput)?;
        let event = OutboxDraft::now(
            Some(tenant.record_id()),
            "computer",
            id.to_string(),
            "computer.reserved",
            1,
            object(&Event {
                computer_id: id,
                actor_key: &owner.principal_key,
                authority: &owner.authority,
            })?,
        );
        let params = vec![
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
            ("event", event.into_value()),
        ];
        self.query(include_str!("../queries/reserve.surql"), params)
            .await?;
        let mut response = self
            .query(
                "SELECT VALUE computer FROM ONLY $request;",
                vec![("request", request.into_value())],
            )
            .await?;
        let selected: Option<RecordId> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut response = self
            .query(
                "SELECT * FROM ONLY $computer;",
                vec![(
                    "computer",
                    selected.ok_or(ComputerError::Unavailable)?.into_value(),
                )],
            )
            .await?;
        let record: Option<crate::model::ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let computer = Computer::try_from(record.ok_or(ComputerError::Unavailable)?)?;
        permits(&computer.owner, owner)?;
        Ok(computer)
    }
}

#[derive(Serialize)]
struct Event<'a> {
    computer_id: Uuid,
    actor_key: &'a str,
    authority: &'a veoveo_mcp_contract::InvocationAuthority,
}

fn object(value: &impl Serialize) -> Result<OpenObject> {
    let serde_json::Value::Object(fields) =
        serde_json::to_value(value).map_err(|_| ComputerError::InvalidInput)?
    else {
        return Err(ComputerError::InvalidInput);
    };
    Ok(OpenObject::new(fields.into_iter().collect()))
}
