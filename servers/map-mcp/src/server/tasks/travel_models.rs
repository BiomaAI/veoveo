//! Completion for caller-owned travel-model results without loading task history.
use anyhow::{Result, ensure};
use veoveo_platform_store::{
    PlatformStore, RecordId, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::TaskOwner;

pub(crate) async fn complete_travel_models(
    store: &PlatformStore,
    owner: &TaskOwner,
    needle: &str,
) -> Result<Vec<String>> {
    ensure!(
        needle.len() <= 512 && !needle.chars().any(char::is_control),
        "invalid completion search text"
    );
    let field = "result.structuredContent.travel_model_id";
    let statement = format!(
        "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND work_context = $context AND (request.owner.tenant_key ?? NONE) = $tenant_key AND request.owner.data_labels ALLINSIDE $labels AND task_type = 'build_travel_model' AND {field} != NONE AND string::lowercase({field} ?? '') CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT 101);"
    );
    let mut response = store
        .client()
        .query(statement)
        .bind(("server", RecordId::new("mcp_server", "map")))
        .bind((
            "tenant",
            deterministic_tenant_id(owner.tenant_key())?.record_id(),
        ))
        .bind((
            "owner",
            deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
        ))
        .bind(("profile", RecordId::new("profile", owner.profile.clone())))
        .bind((
            "context",
            deterministic_work_context_id(
                owner.tenant_key(),
                owner.authority.work_context.as_str(),
            )?
            .record_id(),
        ))
        .bind(("tenant_key", owner.tenant_key.clone()))
        .bind(("labels", owner.data_labels.clone()))
        .bind(("needle", needle.to_lowercase()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, time::Duration};
    use veoveo_mcp_contract::{
        AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
        TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskId, TaskRuntime};

    fn owner(tenant: &str, context: &str, name: &str, profile: &str, labels: &[&str]) -> TaskOwner {
        let principal = PrincipalId::new(name).unwrap();
        TaskOwner {
            principal_key: name.into(),
            principal_kind: PrincipalKind::User,
            issuer: "https://fixture.local".into(),
            subject: name.into(),
            profile: profile.into(),
            tenant_key: Some(tenant.into()),
            data_labels: labels.iter().map(|s| (*s).into()).collect(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new(context).unwrap(),
                tenant: TenantId::new(tenant).unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(principal.clone()),
                    initial_grants: vec![],
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: principal,
                },
            },
        }
    }
    async fn task(runtime: &TaskRuntime, owner: TaskOwner, key: Option<&str>) {
        let id = TaskId::new();
        let principal = veoveo_mcp_contract::PrincipalId::new(owner.principal_key.clone()).unwrap();
        let context = owner.authority.work_context.clone();
        runtime
            .create(CreateTask {
                task_id: id,
                owner,
                server: "map".into(),
                task_type: "build_travel_model".into(),
                request: serde_json::json!({}),
                recovery_class: RecoveryClass::InterruptedIndeterminate,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            })
            .await
            .unwrap();
        if let Some(key) = key {
            let now = chrono::Utc::now();
            let artifact_id = uuid::Uuid::now_v7().to_string().parse().unwrap();
            let uri = format!("artifact://{artifact_id}");
            let record = crate::contract::TravelModelRecord {
                travel_model_id: key.parse().unwrap(),
                travel_model_uri: format!("map://travel-model/{key}"),
                manifest_uri: uri.clone(),
                artifact: veoveo_mcp_contract::ArtifactMetadata {
                    artifact_id,
                    byte_len: 128,
                    mime_type: Some("application/json".into()),
                    filename: None,
                    artifact_uri: uri,
                    download_url: None,
                    created_at: now,
                    release_state: Default::default(),
                    compliance: Default::default(),
                    metadata: serde_json::json!({}),
                },
                cost_metric: Default::default(),
                time_model: Default::default(),
                location_count: 2,
                vehicle_type_count: 1,
                unavailable_cell_count: 0,
                profiles: vec![],
                created_by: principal,
                work_context: context,
                created_at: now,
            };
            let result =
                veoveo_platform_store::OpenObject::new(std::collections::BTreeMap::from([(
                    "structuredContent".into(),
                    serde_json::to_value(record).unwrap(),
                )]));
            runtime
                .platform_store()
                .client()
                .query("UPDATE ONLY $task SET result = $result;")
                .bind(("task", id.record_id()))
                .bind(("result", result))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
    }
    fn key(n: u32) -> String {
        format!("travel-model-{n:08x}-0000-7000-8000-000000000000")
    }

    #[tokio::test]
    async fn travel_completion_filters_task_ownership_and_search_before_its_limit() {
        tokio::time::timeout(Duration::from_secs(120), qualify())
            .await
            .expect("travel completion exceeded 120 seconds");
    }
    async fn qualify() {
        let db = crate::test_store::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "map", "completion-test");
        let reader = owner("map-completion", "operations", "author", "profile-a", &[]);
        for hidden in [
            owner("foreign", "operations", "author", "profile-a", &[]),
            owner("map-completion", "private", "author", "profile-a", &[]),
            owner("map-completion", "operations", "peer", "profile-a", &[]),
            owner("map-completion", "operations", "author", "profile-b", &[]),
            owner(
                "map-completion",
                "operations",
                "author",
                "profile-a",
                &["secret"],
            ),
        ] {
            task(&runtime, hidden, Some(&key(0xffff))).await;
        }
        task(&runtime, reader.clone(), None).await;
        for n in 0..125 {
            task(&runtime, reader.clone(), Some(&key(n))).await;
        }
        task(&runtime, reader.clone(), Some(&key(0))).await;
        let values = complete_travel_models(&db.b, &reader, "").await.unwrap();
        assert_eq!(values.len(), 101);
        assert_eq!(values[100], key(100));
        assert_eq!(
            complete_travel_models(&db.b, &reader, &key(124).to_uppercase())
                .await
                .unwrap(),
            vec![key(124)]
        );
        assert!(
            complete_travel_models(&db.b, &reader, "0000ffff")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            complete_travel_models(&db.b, &reader, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
