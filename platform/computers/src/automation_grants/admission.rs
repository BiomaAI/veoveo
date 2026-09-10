use super::{authority, model};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{
        AutomationGrantCollection, AutomationGrantView, IssueAutomationGrantInput,
        RevokeAutomationGrantInput,
    },
    identity::{digest, owner_key},
    model::computer_record,
    session_grants::object,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{
    OpenObject, PrincipalKind, PrincipalRecord, deterministic_principal_id,
};
use veoveo_policy::PolicyCatalogView;

#[derive(Serialize, SurrealValue)]
struct Content {
    grant_id: Uuid,
    computer_id: Uuid,
    owner_key: String,
    provider_instance_id: Uuid,
    authority: OpenObject,
    grantee: RecordId,
    principal_id: String,
    oauth_client_id: String,
    grantee_issuer: String,
    grantee_subject: String,
    grantee_kind: PrincipalKind,
    name: String,
    permissions: Vec<String>,
    execution_limits: Option<OpenObject>,
    expires_at: DateTime<Utc>,
}

impl ComputersStore {
    pub async fn issue_automation_grant(
        &self,
        actor: &ComputerActor,
        input: &IssueAutomationGrantInput,
    ) -> Result<AutomationGrantView> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.issue_automation(actor, input),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn issue_automation(
        &self,
        actor: &ComputerActor,
        input: &IssueAutomationGrantInput,
    ) -> Result<AutomationGrantView> {
        let principal = model::validate(input)?;
        let owner = self.automation_owner(actor, "grant_automation").await?;
        let computer = self.get(actor.owner(), input.computer_id).await?;
        if computer.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::Forbidden);
        }
        let key = owner_key(actor.owner())?;
        let request = RecordId::new(
            "computer_automation_request",
            digest(&(
                "veoveo.computer.automation.v1",
                &key,
                input.computer_id,
                input.request_id,
            ))?,
        );
        let fingerprint = digest(&(self.provider_instance_id, input))?;
        // Exact retries resolve their original grant even after quota reduction,
        // expiry or revocation; they never recreate its authority.
        #[derive(serde::Deserialize, SurrealValue)]
        struct Prior {
            grant: RecordId,
            fingerprint: String,
        }
        let mut read = self
            .query(
                "SELECT * FROM ONLY $request;",
                vec![("request", request.clone().into_value())],
            )
            .await?;
        let prior: Option<Prior> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        if let Some(prior) = prior {
            if prior.fingerprint != fingerprint {
                return Err(ComputerError::RequestConflict);
            }
            let mut read = self
                .query(
                    "SELECT * FROM ONLY $grant;",
                    vec![("grant", prior.grant.into_value())],
                )
                .await?;
            let row: Option<model::Record> =
                read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let grant: model::Grant = row.ok_or(ComputerError::Unavailable)?.try_into()?;
            authority::owned(&grant, actor, input.computer_id, self.provider_instance_id)?;
            owner.check_fresh(actor)?;
            return Ok(grant.view);
        }
        for permission in &input.permissions {
            authority::require_permission(&owner.snapshot, input.computer_id, *permission)?;
        }
        let profile = owner
            .snapshot
            .catalog
            .profile(&owner.snapshot.accepted.profile)
            .ok_or(ComputerError::Forbidden)?;
        let client = owner
            .snapshot
            .catalog
            .control_plane()
            .oauth_clients
            .iter()
            .find(|client| client.id.as_str() == input.oauth_client_id)
            .ok_or(ComputerError::InvalidInput)?;
        if client.authorization_server != profile.authorization_server
            || !client
                .allowed_resources
                .contains(&profile.protected_resource)
            || client
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &owner.snapshot.accepted.invocation.tenant)
        {
            return Err(ComputerError::InvalidInput);
        }
        let policy = self.stored_automation_policy().await?;
        let limits = policy.checked()?;
        if input.execution_limits.is_some_and(|requested| {
            requested.maximum_seconds > limits.maximum_execution_seconds
                || requested.maximum_output_bytes > limits.maximum_output_bytes
        }) {
            return Err(ComputerError::InvalidInput);
        }
        let grantee_id = deterministic_principal_id(actor.owner().tenant_key(), principal.as_str())
            .map_err(|_| ComputerError::InvalidInput)?
            .record_id();
        let mut read = self
            .query(
                "SELECT * FROM ONLY $grantee;",
                vec![("grantee", grantee_id.clone().into_value())],
            )
            .await?;
        let grantee: Option<PrincipalRecord> =
            read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let grantee = grantee.ok_or(ComputerError::NotFound)?;
        if !grantee.enabled || grantee.id != grantee_id || grantee.tenant != owner.snapshot.tenant {
            return Err(ComputerError::NotFound);
        }
        let grant_id = Uuid::now_v7();
        let content = Content {
            grant_id,
            computer_id: input.computer_id,
            owner_key: key.clone(),
            provider_instance_id: self.provider_instance_id,
            authority: object(actor.accepted())?,
            grantee: grantee.id,
            principal_id: principal.to_string(),
            oauth_client_id: input.oauth_client_id.clone(),
            grantee_issuer: grantee.issuer,
            grantee_subject: grantee.subject,
            grantee_kind: grantee.kind,
            name: input.name.clone(),
            permissions: input
                .permissions
                .iter()
                .map(|p| model::permission_name(*p).to_owned())
                .collect(),
            execution_limits: input.execution_limits.as_ref().map(object).transpose()?,
            expires_at: input.expires_at,
        };
        let mut params = owner.bindings(actor)?;
        params.extend([
            ("computer", computer_record(input.computer_id).into_value()),
            ("computer_id", input.computer_id.into_value()),
            ("owner_key", key.into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("request", request.clone().into_value()),
            ("fingerprint", fingerprint.into_value()),
            ("grant", super::record(grant_id).into_value()),
            ("content", content.into_value()),
            ("policy", self.automation_policy_record().into_value()),
            ("policy_fingerprint", policy.fingerprint.into_value()),
            (
                "maximum_lifetime",
                surrealdb::types::Duration::from_secs(u64::from(limits.maximum_lifetime_seconds))
                    .into_value(),
            ),
            (
                "guard",
                RecordId::new(
                    "computer_automation_guard",
                    surrealdb::types::Uuid::from(input.computer_id),
                )
                .into_value(),
            ),
            (
                "event",
                crate::session_grants::authority::event(
                    actor.accepted(),
                    input.computer_id,
                    grant_id,
                    "automation_granted",
                )?
                .into_value(),
            ),
        ]);
        let mut response = self
            .query(
                include_str!("../../queries/issue_automation_grant.surql"),
                params,
            )
            .await?;
        let index = response
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let row: Option<model::Record> = response
            .take(index)
            .map_err(|_| ComputerError::Unavailable)?;
        let grant: model::Grant = row.ok_or(ComputerError::Unavailable)?.try_into()?;
        authority::owned(&grant, actor, input.computer_id, self.provider_instance_id)?;
        Ok(grant.view)
    }

    pub async fn list_automation_grants(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<AutomationGrantCollection> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            self.get(actor.owner(), computer_id).await?;
            let mut response = self
                .query(
                    "SELECT * FROM computer_automation_grant
                 WHERE owner_key = $owner_key AND computer_id = $computer_id
                   AND revoked_at = NONE AND expires_at > time::now()
                 ORDER BY grant_id LIMIT 64;",
                    vec![
                        ("owner_key", owner_key(actor.owner())?.into_value()),
                        ("computer_id", computer_id.into_value()),
                    ],
                )
                .await?;
            let rows: Vec<model::Record> =
                response.take(0).map_err(|_| ComputerError::Unavailable)?;
            let mut grants = Vec::new();
            for row in rows {
                let grant: model::Grant = row.try_into()?;
                authority::owned(&grant, actor, computer_id, self.provider_instance_id)?;
                grants.push(grant.view);
            }
            control.require_read(Some(computer_id))?;
            Ok(AutomationGrantCollection {
                computer_id,
                grants,
            })
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    pub async fn revoke_automation_grant(
        &self,
        actor: &ComputerActor,
        input: &RevokeAutomationGrantInput,
    ) -> Result<AutomationGrantView> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let owner = self.automation_owner(actor, "revoke_automation").await?;
            self.get(actor.owner(), input.computer_id).await?;
            let grant = self.automation_grant(input.grant_id).await?;
            authority::owned(&grant, actor, input.computer_id, self.provider_instance_id)?;
            let mut params = owner.bindings(actor)?;
            params.extend([
                ("grant", super::record(input.grant_id).into_value()),
                ("computer", computer_record(input.computer_id).into_value()),
                ("computer_id", input.computer_id.into_value()),
                ("owner_key", owner_key(actor.owner())?.into_value()),
                ("provider", self.provider_instance_id.into_value()),
                (
                    "event",
                    crate::session_grants::authority::event(
                        actor.accepted(),
                        input.computer_id,
                        input.grant_id,
                        "automation_revoked",
                    )?
                    .into_value(),
                ),
            ]);
            let mut response = self
                .query(
                    include_str!("../../queries/revoke_automation_grant.surql"),
                    params,
                )
                .await?;
            let index = response
                .num_statements()
                .checked_sub(1)
                .ok_or(ComputerError::Unavailable)?;
            let row: Option<model::Record> = response
                .take(index)
                .map_err(|_| ComputerError::Unavailable)?;
            let grant: model::Grant = row.ok_or(ComputerError::Unavailable)?.try_into()?;
            Ok(grant.view)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
}
