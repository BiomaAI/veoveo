//! Canonical installation-wide tenant, principal, and group identity mapping.

use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    EnterpriseId, EnterpriseRecord, GroupId, GroupRecord, PlatformStore, PrincipalId,
    PrincipalKind, PrincipalRecord, StoreError, TenantId, TenantRecord, WorkContextId,
};

pub(crate) const PLATFORM_ID_NAMESPACE: Uuid =
    Uuid::from_u128(0x7f7b11e2_3b9a_5c7a_9d51_2cf8e1bdfab4);
const MAX_TRANSACTION_ATTEMPTS: u32 = 8;

#[derive(Clone, Debug)]
pub struct PlatformIdentity {
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub tenant_key: String,
    pub principal_key: String,
}

pub fn deterministic_enterprise_id() -> EnterpriseId {
    EnterpriseId::from_uuid(Uuid::new_v5(&PLATFORM_ID_NAMESPACE, b"veoveo-installation"))
}

pub fn deterministic_tenant_id(tenant_key: &str) -> Result<TenantId, StoreError> {
    validate_identity_field("tenant_key", tenant_key, 256)?;
    Ok(TenantId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("tenant:{tenant_key}").as_bytes(),
    )))
}

pub fn deterministic_principal_id(
    tenant_key: &str,
    principal_key: &str,
) -> Result<PrincipalId, StoreError> {
    validate_identity_field("tenant_key", tenant_key, 256)?;
    validate_identity_field("principal_key", principal_key, 512)?;
    Ok(PrincipalId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("principal:{tenant_key}:{principal_key}").as_bytes(),
    )))
}

pub fn deterministic_group_id(tenant_key: &str, group_key: &str) -> Result<GroupId, StoreError> {
    validate_identity_field("tenant_key", tenant_key, 256)?;
    validate_identity_field("group_key", group_key, 512)?;
    Ok(GroupId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("group:{tenant_key}:{group_key}").as_bytes(),
    )))
}

pub fn deterministic_work_context_id(
    tenant_key: &str,
    context_key: &str,
) -> Result<WorkContextId, StoreError> {
    validate_identity_field("tenant_key", tenant_key, 256)?;
    validate_identity_field("context_key", context_key, 256)?;
    Ok(WorkContextId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("work-context:{tenant_key}:{context_key}").as_bytes(),
    )))
}

impl PlatformStore {
    pub async fn ensure_identity(
        &self,
        tenant_key: &str,
        principal_key: &str,
        issuer: &str,
        subject: &str,
        kind: PrincipalKind,
    ) -> Result<PlatformIdentity, StoreError> {
        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            match self
                .ensure_identity_once(tenant_key, principal_key, issuer, subject, kind)
                .await
            {
                Ok(identity) => return Ok(identity),
                Err(StoreError::Database(error))
                    if is_retryable_transaction_failure(&error)
                        && attempt + 1 < MAX_TRANSACTION_ATTEMPTS =>
                {
                    transaction_retry_backoff(attempt).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("the bounded retry loop always returns on its final attempt")
    }

    async fn ensure_identity_once(
        &self,
        tenant_key: &str,
        principal_key: &str,
        issuer: &str,
        subject: &str,
        kind: PrincipalKind,
    ) -> Result<PlatformIdentity, StoreError> {
        let enterprise_id = deterministic_enterprise_id();
        validate_identity_field("issuer", issuer, 2_048)?;
        validate_identity_field("subject", subject, 2_048)?;
        let tenant_id = deterministic_tenant_id(tenant_key)?;
        let principal_id = deterministic_principal_id(tenant_key, principal_key)?;
        let now = Utc::now();
        let mut existing = self
            .db
            .query("SELECT * FROM ONLY $enterprise; SELECT * FROM ONLY $tenant; SELECT * FROM ONLY $principal;")
            .bind(("enterprise", enterprise_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("principal", principal_id.record_id()))
            .await?
            .check()?;
        let existing_enterprise = existing.take::<Option<EnterpriseRecord>>(0)?;
        let existing_tenant = existing.take::<Option<TenantRecord>>(1)?;
        let existing_principal = existing.take::<Option<PrincipalRecord>>(2)?;
        if let Some(record) = &existing_tenant {
            validate_existing_tenant(record, enterprise_id, tenant_key)?;
        }
        if let Some(record) = &existing_principal {
            validate_existing_principal(record, tenant_id, principal_key, issuer, subject, kind)?;
        }
        let enterprise = existing_enterprise.map_or_else(
            || EnterpriseRecord {
                id: enterprise_id.record_id(),
                slug: "installation".into(),
                name: "Veoveo installation".into(),
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            |mut record| {
                record.updated_at = now;
                record
            },
        );
        let tenant = existing_tenant.map_or_else(
            || TenantRecord {
                id: tenant_id.record_id(),
                enterprise: enterprise_id.record_id(),
                slug: tenant_key.to_owned(),
                name: tenant_key.to_owned(),
                classification_ceiling: "installation_policy".into(),
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            |mut record| {
                record.updated_at = now;
                record
            },
        );
        let principal = existing_principal.map_or_else(
            || PrincipalRecord {
                id: principal_id.record_id(),
                tenant: tenant_id.record_id(),
                kind,
                issuer: issuer.to_owned(),
                subject: subject.to_owned(),
                display_name: principal_key.to_owned(),
                email: None,
                claims_hash: String::new(),
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            |mut record| {
                record.display_name = principal_key.to_owned();
                record.updated_at = now;
                record
            },
        );
        self.db
            .query("BEGIN TRANSACTION; UPSERT ONLY $enterprise CONTENT $enterprise_content RETURN NONE; UPSERT ONLY $tenant CONTENT $tenant_content RETURN NONE; UPSERT ONLY $principal CONTENT $principal_content RETURN NONE; COMMIT TRANSACTION;")
            .bind(("enterprise", enterprise_id.record_id()))
            .bind(("enterprise_content", enterprise))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("tenant_content", tenant))
            .bind(("principal", principal_id.record_id()))
            .bind(("principal_content", principal))
            .await?
            .check()?;
        Ok(PlatformIdentity {
            tenant_id,
            principal_id,
            tenant_key: tenant_key.to_owned(),
            principal_key: principal_key.to_owned(),
        })
    }

    pub async fn ensure_group(
        &self,
        identity: &PlatformIdentity,
        group_key: &str,
    ) -> Result<GroupId, StoreError> {
        let id = deterministic_group_id(&identity.tenant_key, group_key)?;
        let now = Utc::now();
        let created_at = self
            .db
            .select::<Option<GroupRecord>>(id.record_id())
            .await?
            .map_or(now, |record| record.created_at);
        let content = GroupRecord {
            id: id.record_id(),
            tenant: identity.tenant_id.record_id(),
            external_id: group_key.to_owned(),
            display_name: group_key.to_owned(),
            created_at,
            updated_at: now,
        };
        self.db
            .query("UPSERT ONLY $record CONTENT $content RETURN NONE;")
            .bind(("record", id.record_id()))
            .bind(("content", content))
            .await?
            .check()?;
        Ok(id)
    }
}

fn is_retryable_transaction_failure(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
        || error
            .message()
            .contains("not executed due to a failed transaction")
}

async fn transaction_retry_backoff(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(1_u64 << attempt)).await;
}

fn validate_identity_field(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::InvalidIdentityField {
            field,
            reason: "must not be empty",
        });
    }
    if value.len() > max_bytes {
        return Err(StoreError::InvalidIdentityField {
            field,
            reason: "exceeds maximum encoded length",
        });
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::InvalidIdentityField {
            field,
            reason: "must not contain control characters",
        });
    }
    Ok(())
}

fn validate_existing_tenant(
    record: &TenantRecord,
    enterprise_id: EnterpriseId,
    tenant_key: &str,
) -> Result<(), StoreError> {
    if record.enterprise != enterprise_id.record_id() || record.slug != tenant_key {
        return Err(StoreError::IdentityConflict {
            entity: "tenant",
            key: tenant_key.to_owned(),
        });
    }
    Ok(())
}

fn validate_existing_principal(
    record: &PrincipalRecord,
    tenant_id: TenantId,
    principal_key: &str,
    issuer: &str,
    subject: &str,
    kind: PrincipalKind,
) -> Result<(), StoreError> {
    if record.tenant != tenant_id.record_id()
        || record.display_name != principal_key
        || record.issuer != issuer
        || record.subject != subject
        || record.kind != kind
    {
        return Err(StoreError::IdentityConflict {
            entity: "principal",
            key: principal_key.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_identity_is_tenant_scoped_and_stable() {
        let acme_alice = deterministic_principal_id("acme", "alice").unwrap();
        let beta_alice = deterministic_principal_id("beta", "alice").unwrap();
        assert_ne!(
            deterministic_tenant_id("acme").unwrap(),
            deterministic_tenant_id("beta").unwrap()
        );
        assert_ne!(acme_alice, beta_alice);
        assert_eq!(
            acme_alice,
            deterministic_principal_id("acme", "alice").unwrap()
        );
    }

    #[test]
    fn rejects_invalid_and_conflicting_identity_material() {
        assert!(matches!(
            deterministic_tenant_id("  "),
            Err(StoreError::InvalidIdentityField {
                field: "tenant_key",
                ..
            })
        ));
        let tenant_id = deterministic_tenant_id("acme").unwrap();
        let record = PrincipalRecord {
            id: deterministic_principal_id("acme", "alice")
                .unwrap()
                .record_id(),
            tenant: tenant_id.record_id(),
            kind: PrincipalKind::User,
            issuer: "https://issuer.example".into(),
            subject: "alice-subject".into(),
            display_name: "alice".into(),
            email: None,
            claims_hash: String::new(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(matches!(
            validate_existing_principal(
                &record,
                tenant_id,
                "alice",
                "https://attacker.example",
                "alice-subject",
                PrincipalKind::User,
            ),
            Err(StoreError::IdentityConflict {
                entity: "principal",
                ..
            })
        ));
    }
}
