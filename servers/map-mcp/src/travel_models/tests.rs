use super::*;
use std::{collections::BTreeSet, time::Duration};
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskRuntime};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

fn owner(tenant: &str, context: &str, name: &str, profile: &str, labels: &[&str]) -> TaskOwner {
    let principal = PrincipalId::parse(name).unwrap();
    TaskOwner {
        principal_key: name.into(),
        principal_kind: PrincipalKind::User,
        issuer: "https://fixture.local".into(),
        subject: name.into(),
        profile: profile.into(),
        tenant_key: Some(tenant.into()),
        data_labels: labels.iter().map(|s| (*s).into()).collect(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse(context).unwrap(),
            tenant: TenantId::parse(tenant).unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
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

async fn update(runtime: &TaskRuntime, id: TaskId, sql: &str) {
    runtime
        .platform_store()
        .client()
        .query(sql)
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn travel_reads_exclude_inconsistent_authority_and_unfinished_results_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let runtime =
            crate::task_lookup::bind(TaskRuntime::new(db.a.clone(), "map", "writer")).unwrap();
        let reads = TravelModelReads::new(&db.b);
        let caller = owner("tenant", "context", "owner", "profile", &[]);
        for (number, mutation) in [
            include_str!("../queries/travel_models/tests/mutation_01.surql"),
            include_str!("../queries/travel_models/tests/mutation_02.surql"),
            include_str!("../queries/travel_models/tests/mutation_03.surql"),
            include_str!("../queries/travel_models/tests/mutation_04.surql"),
            include_str!("../queries/travel_models/tests/mutation_05.surql"),
            include_str!("../queries/travel_models/tests/mutation_06.surql"),
            include_str!("../queries/travel_models/tests/mutation_07.surql"),
            include_str!("../queries/travel_models/tests/mutation_08.surql"),
            include_str!("../queries/travel_models/tests/mutation_09.surql"),
            include_str!("../queries/travel_models/tests/mutation_10.surql"),
            include_str!("../queries/travel_models/tests/mutation_11.surql"),
            include_str!("../queries/travel_models/tests/mutation_12.surql"),
            include_str!("../queries/travel_models/tests/mutation_13.surql"),
            include_str!("../queries/travel_models/tests/mutation_14.surql"),
            include_str!("../queries/travel_models/tests/mutation_15.surql"),
            include_str!("../queries/travel_models/tests/mutation_16.surql"),
            include_str!("../queries/travel_models/tests/mutation_17.surql"),
            include_str!("../queries/travel_models/tests/mutation_18.surql"),
            include_str!("../queries/travel_models/tests/mutation_19.surql"),
            include_str!("../queries/travel_models/tests/mutation_20.surql"),
            include_str!("../queries/travel_models/tests/mutation_21.surql"),
            include_str!("../queries/travel_models/tests/mutation_22.surql"),
            include_str!("../queries/travel_models/tests/mutation_23.surql"),
            include_str!("../queries/travel_models/tests/mutation_24.surql"),
            include_str!("../queries/travel_models/tests/mutation_25.surql"),
            include_str!("../queries/travel_models/tests/mutation_26.surql"),
            include_str!("../queries/travel_models/tests/mutation_27.surql"),
            include_str!("../queries/travel_models/tests/mutation_28.surql"),
        ]
        .into_iter()
        .enumerate()
        {
            let model: TravelModelId = key(number as u32).parse().unwrap();
            let id = task(&runtime, caller.clone(), Some(model.as_str())).await;
            update(&runtime, id, mutation).await;
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
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let runtime =
            crate::task_lookup::bind(TaskRuntime::new(db.a.clone(), "map", "writer")).unwrap();
        let reads = TravelModelReads::new(&db.b);
        let explicit = owner("installation", "context", "owner", "profile", &[]);
        let mut implicit = explicit.clone();
        implicit.tenant_key = None;
        for (number, (allowed, denied)) in [(&explicit, &implicit), (&implicit, &explicit)]
            .into_iter()
            .enumerate()
        {
            let model: TravelModelId = key(number as u32).parse().unwrap();
            let id = task(&runtime, allowed.clone(), Some(model.as_str())).await;
            assert!(reads.get(allowed, &model).await.unwrap().is_some());
            assert!(reads.get(denied, &model).await.unwrap().is_none());
            assert!(
                reads
                    .complete(denied, model.as_str())
                    .await
                    .unwrap()
                    .is_empty()
            );
            update(
                &runtime,
                id,
                include_str!("../queries/travel_models/tests/mutation_29.surql"),
            )
            .await;
            assert!(reads.get(allowed, &model).await.unwrap().is_none());
            let mut cleared = allowed.clone();
            cleared.data_labels.insert("secret".into());
            assert!(reads.get(&cleared, &model).await.unwrap().is_some());
            cleared.authority.tenant = TenantId::parse("other").unwrap();
            assert!(reads.get(&cleared, &model).await.is_err());
        }
        let model: TravelModelId = key(30).parse().unwrap();
        let id = task(&runtime, explicit.clone(), Some(model.as_str())).await;
        update(
            &runtime,
            id,
            include_str!("../queries/travel_models/tests/mutation_30.surql"),
        )
        .await;
        assert!(reads.get(&explicit, &model).await.is_err());
        runtime
            .platform_store()
            .client()
            .query(include_str!(
                "../queries/travel_models/tests/mutation_31.surql"
            ))
            .bind(("task", veoveo_platform_store::task_record_id(id)))
            .bind((
                "model_uri",
                crate::contract::MapTravelModelUri::new(model.clone()).to_string(),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(reads.get(&explicit, &model).await.is_err());
        update(
            &runtime,
            id,
            include_str!("../queries/travel_models/tests/mutation_32.surql"),
        )
        .await;
        assert!(reads.complete(&explicit, "malformed").await.is_err());
        assert!(reads.complete(&explicit, "\n").await.is_err());
        assert!(reads.complete(&explicit, &"x".repeat(513)).await.is_err());
    })
    .await
    .expect("travel-model retained-data qualification exceeded 90 seconds");
}
async fn task(runtime: &TaskRuntime, owner: TaskOwner, key: Option<&str>) -> TaskId {
    let id = TaskId::new();
    let principal = veoveo_types::PrincipalId::parse(owner.principal_key.clone()).unwrap();
    let context = owner.authority.work_context.clone();
    let now = chrono::Utc::now();
    let input_cases: serde_json::Value =
        serde_json::from_str(include_str!("../../testdata/controlled-inputs.json")).unwrap();
    let identity = veoveo_mcp_contract::GatewayInternalIdentity {
        issuer: "veoveo-internal".parse().unwrap(),
        profile: owner.profile.parse().unwrap(),
        server: "map".parse().unwrap(),
        actor: veoveo_mcp_contract::Principal {
            id: owner.principal_key.parse().unwrap(),
            kind: veoveo_mcp_contract::PrincipalKind::User,
            issuer: owner.issuer.parse().unwrap(),
            subject: owner.subject.parse().unwrap(),
            tenant: owner.tenant_key.as_ref().map(|v| v.parse().unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: owner
                .data_labels
                .iter()
                .map(|v| v.parse().unwrap())
                .collect(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(now),
        },
        authority: owner.authority.clone(),
        request_context: None,
        jwt_id: "fixture".parse().unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + chrono::TimeDelta::hours(1),
    };
    let request = crate::task_lookup::DurableTravelModelRequest {
        input: serde_json::from_value(input_cases[0]["arguments"].clone()).unwrap(),
        identity, travel_model_id: key.map(|v|v.parse().unwrap()).unwrap_or_default(), created_at:now,
        artifact_write_capability: serde_json::from_value(serde_json::json!({"capability_id":uuid::Uuid::now_v7().to_string(),"secret":"inert_fixture_capability_not_issued_000000000000","task_id":id.to_string(),"expires_at":now+chrono::TimeDelta::hours(1)})).unwrap(),
    };
    runtime
        .create(CreateTask {
            task_id: id,
            owner: owner.clone(),
            server: "map".into(),
            task_type: MapTaskKind::BuildTravelModel.name(),
            request: serde_json::json!({"kind":"build_travel_model","request":request}),
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
        let uri = veoveo_types::ResourceAddress::to_uri(&record.travel_model_uri).unwrap();
        let product = crate::contract::MapTaskProduct::new(record).unwrap();
        let mut wrong = serde_json::to_value(&product).unwrap();
        wrong["result_uri"] = serde_json::json!(crate::contract::MapTravelModelUri::new(
            self::key(999).parse().unwrap()
        ));
        assert!(
            serde_json::from_value::<
                crate::contract::MapTaskProduct<crate::contract::TravelModelRecord>,
            >(wrong)
            .is_err()
        );
        let result = veoveo_mcp_contract::hosting::product_result(
            "Travel model built",
            rmcp::model::Resource::new(uri.as_str(), "Travel model"),
            &product,
        )
        .unwrap();
        runtime.claim(id, Duration::from_secs(30)).await.unwrap();
        runtime
            .transition(
                id,
                veoveo_task_runtime::mcp_task_completion("fixture", result).unwrap(),
            )
            .await
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
    let db = crate::test_store::TestDb::with_modules(vec![
        crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let runtime =
        crate::task_lookup::bind(TaskRuntime::new(db.a.clone(), "map", "completion-test")).unwrap();
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
            include_str!("../queries/travel_models/tests/mutation_33.surql"),
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
        include_str!("../queries/travel_models/tests/mutation_34.surql"),
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
    update(
        &runtime,
        expected[124].0,
        include_str!("../queries/travel_models/tests/mutation_35.surql"),
    )
    .await;
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

#[tokio::test]
async fn retained_input_policy_and_lookup_integrity_survive_task_changes() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let runtime =
            crate::task_lookup::bind(TaskRuntime::new(db.a.clone(), "map", "lookup-controls"))
                .unwrap();
        let reads = TravelModelReads::new(&db.b);
        let owner = owner("lookup", "context", "owner", "profile", &["secret"]);
        let model: TravelModelId = key(900).parse().unwrap();
        let id = task(&runtime, owner.clone(), Some(model.as_str())).await;
        update(
            &runtime,
            id,
            include_str!("../queries/travel_models/tests/changed_profile.surql"),
        )
        .await;
        let mut changed = owner.clone();
        changed.profile = "changed-profile".into();
        assert!(reads.get(&changed, &model).await.unwrap().is_none());
        assert!(reads.complete(&changed, "").await.unwrap().is_empty());
        let model: TravelModelId = key(901).parse().unwrap();
        let id = task(&runtime, owner.clone(), Some(model.as_str())).await;
        update(
            &runtime,
            id,
            include_str!("../queries/travel_models/tests/lowered_clearance.surql"),
        )
        .await;
        let mut lowered = owner.clone();
        lowered.data_labels.clear();
        assert!(reads.get(&lowered, &model).await.unwrap().is_none());
        assert!(
            reads
                .page(&lowered, &MapTravelModelsUri::new(None))
                .await
                .unwrap()
                .items
                .is_empty()
        );
        let model: TravelModelId = key(902).parse().unwrap();
        let id = task(&runtime, owner.clone(), Some(model.as_str())).await;
        let lookup = RecordId::new("map_travel_model_task", id.to_string());
        let mut response =
            db.a.client()
                .query(include_str!(
                    "../queries/travel_models/tests/read_lookup.surql"
                ))
                .bind(("lookup", lookup.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let original: Value = response.take(0).unwrap();
        for case in 0..3 {
            let mut content = original.clone();
            let Value::Object(ref mut row) = content else {
                unreachable!()
            };
            let Value::Object(identity) = row.get_mut("identity").unwrap() else {
                unreachable!()
            };
            match case {
                0 => {
                    identity.remove("profile");
                }
                1 => {
                    identity.insert("unknown", true.into_value());
                }
                _ => {
                    identity.remove("expected_input");
                }
            }
            assert!(
                db.a.client()
                    .query(include_str!(
                        "../queries/travel_models/tests/write_lookup.surql"
                    ))
                    .bind(("lookup", lookup.clone()))
                    .bind(("content", content))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "lookup control {case}"
            );
            let mut response =
                db.b.client()
                    .query(include_str!(
                        "../queries/travel_models/tests/read_lookup.surql"
                    ))
                    .bind(("lookup", lookup.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            assert_eq!(response.take::<Value>(0).unwrap(), original);
        }
        let wrong: TravelModelId = key(903).parse().unwrap();
        let mut content = original;
        let Value::Object(ref mut row) = content else {
            unreachable!()
        };
        let Value::Object(identity) = row.get_mut("identity").unwrap() else {
            unreachable!()
        };
        identity.insert("travel_model_id", wrong.to_string().into_value());
        db.a.client()
            .query(include_str!(
                "../queries/travel_models/tests/write_lookup.surql"
            ))
            .bind(("lookup", lookup))
            .bind(("content", content))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(reads.get(&owner, &wrong).await.is_err());
        assert!(
            reads
                .page(&owner, &MapTravelModelsUri::new(None))
                .await
                .is_err()
        );
        assert!(reads.complete(&owner, wrong.as_str()).await.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn terminal_contributions_distinguish_tool_error_failure_and_cancellation() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let runtime =
            crate::task_lookup::bind(TaskRuntime::new(db.a.clone(), "map", "terminal-controls"))
                .unwrap();
        let caller = owner("terminal", "context", "owner", "profile", &[]);
        for (transition, outcome) in [
            (
                veoveo_task_runtime::TaskTransition::Succeeded {
                    message: "tool error".into(),
                    result: serde_json::json!({"content":[],"isError":true}),
                    result_uri: None,
                },
                crate::task_lookup::Outcome::ToolError,
            ),
            (
                veoveo_task_runtime::TaskTransition::Failed(veoveo_task_runtime::TaskFailure::new(
                    "fixture",
                    "known failure",
                )),
                crate::task_lookup::Outcome::Failed,
            ),
            (
                veoveo_task_runtime::TaskTransition::Cancelled,
                crate::task_lookup::Outcome::Cancelled,
            ),
        ] {
            let id = task(&runtime, caller.clone(), None).await;
            runtime.claim(id, Duration::from_secs(30)).await.unwrap();
            if matches!(transition, veoveo_task_runtime::TaskTransition::Cancelled) {
                runtime.cancel(id).await.unwrap();
            }
            runtime.transition(id, transition).await.unwrap();
            let mut response =
                db.a.client()
                    .query(include_str!(
                        "../queries/travel_models/tests/read_lookup.surql"
                    ))
                    .bind((
                        "lookup",
                        RecordId::new("map_travel_model_task", id.to_string()),
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let row =
                crate::task_lookup::Row::from_value(response.take::<Value>(0).unwrap()).unwrap();
            assert!(row.settlement.outcome == outcome);
            assert!(row.settlement.expected_result.is_none());
        }
        let reads = TravelModelReads::new(&db.b);
        assert!(reads.complete(&caller, "").await.unwrap().is_empty());
        assert!(
            reads
                .page(&caller, &MapTravelModelsUri::new(None))
                .await
                .unwrap()
                .items
                .is_empty()
        );
    })
    .await
    .unwrap();
}
