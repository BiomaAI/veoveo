use super::*;
use std::{collections::BTreeSet, time::Duration};
use veoveo_mcp_contract::{
    InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskRuntime};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};

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

async fn update(runtime: &TaskRuntime, id: TaskId, fields: &str) {
    runtime
        .platform_store()
        .client()
        .query(format!("UPDATE ONLY $task SET {fields} RETURN NONE;"))
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn travel_reads_exclude_inconsistent_authority_and_unfinished_results_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "map", "writer");
        let reads = TravelModelReads::new(&db.b);
        let caller = owner("tenant", "context", "owner", "profile", &[]);
        for (number, mutation) in [
            "tenant = tenant:other",
            "owner = principal:other",
            "profile = profile:other",
            "work_context = work_context:other",
            "server = mcp_server:other",
            "request.owner.principal_key = 'other'",
            "request.owner.profile = 'other'",
            "request.owner.tenant_key = NONE",
            "request.owner.data_labels = ['secret']",
            "authority.context_key = 'other'",
            "request.owner.authority.work_context = 'other'",
            "request.owner.authority.tenant = 'other'",
            "request.input.kind = 'route'",
            "request.input.request.identity.actor.id = 'other'",
            "request.input.request.identity.actor.tenant = NONE",
            "request.input.request.identity.actor.data_labels = ['secret']",
            "request.input.request.identity.profile = 'other'",
            "request.input.request.identity.authority.work_context = 'other'",
            "request.input.request.identity.authority.tenant = 'other'",
            "result.payload.structuredContent.created_by = 'other'",
            "result.payload.structuredContent.work_context = 'other'",
            "request.input.request.travel_model_id = 'other'",
            "task_type = 'route'",
            "status = 'queued'",
            "status = 'failed'",
            "status = 'cancelled'",
            "result.payload.isError = true",
            "result.payload.isError = 'false'",
        ]
        .into_iter()
        .enumerate()
        {
            let model: TravelModelId = key(number as u32).parse().unwrap();
            let id = task(&runtime, caller.clone(), Some(model.as_str())).await;
            update(
                &runtime,
                id,
                &format!("{mutation}, result.payload.structuredContent.created_at = 'malformed'"),
            )
            .await;
            assert!(
                reads.get(&caller, &model).await.unwrap().is_none(),
                "selected {mutation}"
            );
            assert!(
                reads
                    .page(&caller, &MapTravelModelsUri::new(None))
                    .await
                    .unwrap()
                    .items
                    .is_empty(),
                "page selected {mutation}"
            );
            assert!(
                reads.complete(&caller, "").await.unwrap().is_empty(),
                "completion selected {mutation}"
            );
        }
    })
    .await
    .expect("travel-model ownership qualification exceeded 90 seconds");
}

#[tokio::test]
async fn optional_tenants_clearance_and_selected_parent_corruption_are_checked() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "map", "writer");
        let reads = TravelModelReads::new(&db.b);
        let explicit = owner("installation", "context", "owner", "profile", &[]);
        let mut implicit = explicit.clone();
        implicit.tenant_key = None;
        for (number, (allowed, denied)) in [(&explicit, &implicit), (&implicit, &explicit)].into_iter().enumerate() {
            let model: TravelModelId = key(number as u32).parse().unwrap();
            let id = task(&runtime, allowed.clone(), Some(model.as_str())).await;
            assert!(reads.get(allowed, &model).await.unwrap().is_some());
            assert!(reads.get(denied, &model).await.unwrap().is_none());
            assert!(reads.complete(denied, model.as_str()).await.unwrap().is_empty());
            update(&runtime, id, "request.owner.data_labels = ['secret']").await;
            assert!(reads.get(allowed, &model).await.unwrap().is_none());
            let mut cleared = allowed.clone();
            cleared.data_labels.insert("secret".into());
            assert!(reads.get(&cleared, &model).await.unwrap().is_some());
            cleared.authority.tenant = TenantId::new("other").unwrap();
            assert!(reads.get(&cleared, &model).await.is_err());
        }
        let model: TravelModelId = key(30).parse().unwrap();
        let id = task(&runtime, explicit.clone(), Some(model.as_str())).await;
        update(&runtime, id, "result.payload.structuredContent.travel_model_uri = 'map://travel-model/travel-model-0195dabe-7777-7abc-8def-ffffffffffff'").await;
        assert!(reads.get(&explicit, &model).await.is_err());
        update(&runtime, id, &format!("result.payload.structuredContent.travel_model_uri = 'map://travel-model/{}', result.payload.structuredContent.manifest_uri = 'artifact://0195dabe-7777-7abc-8def-ffffffffffff'", model)).await;
        assert!(reads.get(&explicit, &model).await.is_err());
        update(&runtime, id, "request.input.request.travel_model_id = 'malformed', result.payload.structuredContent.travel_model_id = 'malformed'").await;
        assert!(reads.complete(&explicit, "malformed").await.is_err());
        assert!(reads.complete(&explicit, "\n").await.is_err());
        assert!(reads.complete(&explicit, &"x".repeat(513)).await.is_err());
    }).await.expect("travel-model retained-data qualification exceeded 90 seconds");
}
async fn task(runtime: &TaskRuntime, owner: TaskOwner, key: Option<&str>) -> TaskId {
    let id = TaskId::new();
    let principal = veoveo_types::PrincipalId::new(owner.principal_key.clone()).unwrap();
    let context = owner.authority.work_context.clone();
    runtime
            .create(CreateTask {
                task_id: id,
                owner: owner.clone(),
                server: "map".into(),
                task_type: const { veoveo_types::TaskTypeName::from_static("build_travel_model") },
                // Only retained fields consumed by this reader; this fixture never runs a worker.
                request: serde_json::json!({"kind":"build_travel_model", "request": {
                    "travel_model_id":key,
                    "identity":{"actor":{"id":owner.principal_key,"tenant":owner.tenant_key,"data_labels":owner.data_labels},
                        "profile":owner.profile,"authority":owner.authority}
                }}),
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
        let artifact_id = veoveo_artifact_contract::ArtifactId::new();
        let uri = artifact_id.plane_uri();
        let record = crate::contract::TravelModelRecord {
            travel_model_id: key.parse().unwrap(),
            travel_model_uri: crate::contract::MapTravelModelUri::new(key.parse().unwrap()),
            manifest_uri: uri.clone(),
            artifact: veoveo_artifact_contract::ArtifactMetadata {
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
        let result = veoveo_platform_store::TaskResultRecord::new(serde_json::json!({
            "structuredContent": serde_json::to_value(record).unwrap(),
        }));
        runtime
            .platform_store()
            .client()
            .query("UPDATE ONLY $task SET status = 'succeeded', result = $result;")
            .bind(("task", veoveo_platform_store::task_record_id(id)))
            .bind(("result", result))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    id
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
    let reads = TravelModelReads::new(&db.b);
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
    for n in 200..=304 {
        let denied = task(
            &runtime,
            owner("map-completion", "operations", "peer", "profile-a", &[]),
            Some(&key(n)),
        )
        .await;
        update(
            &runtime,
            denied,
            "result.payload.structuredContent.created_at = 'malformed'",
        )
        .await;
    }
    task(&runtime, reader.clone(), None).await;
    let mut expected = Vec::new();
    for n in 0..125 {
        let task = task(&runtime, reader.clone(), Some(&key(n))).await;
        expected.push((task, key(n).parse::<TravelModelId>().unwrap()));
    }
    expected.sort_by_key(|(task, _)| *task);
    let first = reads
        .page(&reader, &MapTravelModelsUri::new(None))
        .await
        .unwrap();
    assert_eq!(first.items.len(), 100);
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| &item.travel_model_id)
            .collect::<Vec<_>>(),
        expected[..100].iter().map(|(_, id)| id).collect::<Vec<_>>()
    );
    let continuation = MapTravelModelsUri::new(first.next_cursor);
    let next = reads.page(&reader, &continuation).await.unwrap();
    assert_eq!(next.items.len(), 25);
    assert!(next.next_cursor.is_none());
    assert_eq!(
        next.items
            .iter()
            .map(|item| &item.travel_model_id)
            .collect::<Vec<_>>(),
        expected[100..].iter().map(|(_, id)| id).collect::<Vec<_>>()
    );
    update(
        &runtime,
        expected[124].0,
        "request.owner.data_labels = ['secret']",
    )
    .await;
    assert_eq!(
        reads
            .page(&reader, &continuation)
            .await
            .unwrap()
            .items
            .len(),
        24
    );
    let mut cleared = reader.clone();
    cleared.data_labels.insert("secret".into());
    assert_eq!(
        reads
            .page(&cleared, &continuation)
            .await
            .unwrap()
            .items
            .len(),
        25
    );
    update(&runtime, expected[124].0, "request.owner.data_labels = []").await;
    task(&runtime, reader.clone(), Some(&key(0))).await;
    assert_eq!(
        reads
            .get(&reader, &key(124).parse().unwrap())
            .await
            .unwrap()
            .unwrap()
            .travel_model_id
            .as_str(),
        key(124)
    );
    assert!(reads.get(&reader, &key(0).parse().unwrap()).await.is_err());
    let values = reads.complete(&reader, "").await.unwrap();
    assert_eq!(values.len(), 101);
    assert_eq!(values[100].as_str(), key(100));
    assert_eq!(
        reads
            .complete(&reader, &key(124).to_uppercase())
            .await
            .unwrap(),
        vec![key(124).parse::<TravelModelId>().unwrap()]
    );
    assert!(
        reads
            .complete(&reader, "0000ffff")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        reads
            .complete(&reader, "' OR true --")
            .await
            .unwrap()
            .is_empty()
    );
}
