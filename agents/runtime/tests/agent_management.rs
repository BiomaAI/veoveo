//! Real database checks for publication, authority fencing and concurrent authoring.
use veoveo_agent_runtime::persistence::AgentRepository;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "agent_management/instances.rs"]
mod instances;

use fixture::TestDb;
use uuid::Uuid;
use veoveo_agent_runtime::persistence::*;
use veoveo_platform_store::{PlatformStore, WorkContextMembershipLevel};

#[path = "agent_management/support.rs"]
mod support;
use support::*;

#[tokio::test]
async fn publication_pins_content_and_catalog_never_discloses_instructions() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    let bob = identity(&db.a, "catalog-test", "bob").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let initial = create(&db.a, &a, "researcher").await;
    assert!(
        AgentRepository::new(db.b.clone())
            .agent_catalog(&b, None, 20)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_definition(&b, "researcher")
            .await,
        Err(AgentManagementError::NotFound)
    );
    {
        use surrealdb::types::{SurrealValue, Value};
        let mut response =
            db.a.client()
                .query(include_str!(
                    "queries/agent_management/closed_records/read.surql"
                ))
                .bind(("record", initial.id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let original: Value = response.take(0).unwrap();
        for case in 0..3 {
            let mut content = original.clone();
            let Value::Object(ref mut fields) = content else {
                unreachable!()
            };
            match case {
                0 => {
                    fields.remove("draft_execution");
                }
                1 => {
                    let Value::Object(model) = fields.get_mut("draft_model").unwrap() else {
                        unreachable!()
                    };
                    model.insert("unknown", true.into_value());
                }
                _ => {
                    let Value::Object(execution) = fields.get_mut("draft_execution").unwrap()
                    else {
                        unreachable!()
                    };
                    execution.insert("kind", "invented".into_value());
                }
            }
            assert!(
                db.a.client()
                    .query(include_str!(
                        "queries/agent_management/closed_records/write.surql"
                    ))
                    .bind(("record", initial.id.clone()))
                    .bind(("content", content))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "draft projection control {case}"
            );
            assert_eq!(
                AgentRepository::new(db.b.clone())
                    .agent_definition(&a, "researcher")
                    .await
                    .unwrap(),
                initial
            );
        }
    }
    let first = publish(&db.a, &a, &initial).await;
    assert!(first.published.is_some());
    let revision = AgentRepository::new(db.b.clone())
        .agent_authored_revision(&a, "researcher", &initial.draft_digest)
        .await
        .unwrap();
    assert_eq!(revision.execution, initial.draft.execution);
    assert_eq!(revision.model, initial.draft.model);
    assert_eq!(revision.tools, initial.draft.tools);
    assert_eq!(revision.template_revision, None);
    assert_eq!(initial.draft_execution, initial.draft.execution);
    assert_eq!(initial.draft_model, initial.draft.model);
    assert_eq!(initial.draft_tools, initial.draft.tools);
    assert_eq!(first.audience, vec![a.work_context.clone()]);
    let catalog = AgentRepository::new(db.b.clone())
        .agent_catalog(&b, None, 20)
        .await
        .unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].digest, initial.draft.digest().unwrap());
    let immutable =
        db.a.client()
            .query(include_str!("queries/agent_management/publication_pins_content_and_catalog_never_discloses_instructions/statement_1.surql"))
            .bind(("revision", first.published.clone().unwrap()))
            .await
            .unwrap()
            .check();
    assert!(
        immutable.is_err(),
        "published executable content must be immutable"
    );
    let public = serde_json::to_string(&catalog).unwrap();
    assert!(!public.contains("instructions"));
    assert!(!public.contains("Private instructions"));
    let old_digest = catalog[0].digest.clone();
    let draft = AgentRepository::new(db.b.clone())
        .mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Private instructions v2"),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .agent_catalog(&b, None, 20)
            .await
            .unwrap()[0]
            .digest,
        old_digest
    );
    let forged_digest = AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Publish {
                digest: old_digest.clone(),
                audience: audience(&a),
            },
        )
        .await;
    assert_eq!(forged_digest, Err(AgentManagementError::Conflict));
    let second = publish(&db.a, &a, &draft).await;
    assert_ne!(
        AgentRepository::new(db.b.clone())
            .agent_catalog(&b, None, 20)
            .await
            .unwrap()[0]
            .digest,
        old_digest
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_revision(&b, "researcher", &old_digest)
            .await
            .unwrap()
            .content,
        initial.draft
    );
    let archived = AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(second.revision),
            AgentDefinitionMutation::Status {
                status: AgentDefinitionStatus::Archived,
            },
        )
        .await
        .unwrap();
    assert!(
        AgentRepository::new(db.b.clone())
            .agent_catalog(&b, None, 20)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        AgentRepository::new(db.b.clone())
            .agent_revision(&b, "researcher", &old_digest)
            .await
            .is_ok()
    );
    AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(archived.revision),
            AgentDefinitionMutation::Status {
                status: AgentDefinitionStatus::Disabled,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_revision(&b, "researcher", &old_digest)
            .await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .agent_revision_history(&a, "researcher", None, 100)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn mutation_replay_conflicts_and_concurrent_edits_preserve_one_result() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let request = Uuid::now_v7();
    let create = AgentDefinitionMutation::Create {
        name: "Original".into(),
        description: "A test definition".into(),
        content: content("Instruction"),
    };
    let repository_a = AgentRepository::new(db.a.clone());
    let repository_b = AgentRepository::new(db.b.clone());
    let (first, replay) = tokio::join!(
        repository_a.mutate_agent_definition(&a, "writer", request, None, create.clone()),
        repository_b.mutate_agent_definition(&a, "writer", request, None, create.clone())
    );
    let first = first.unwrap();
    assert_eq!(replay.unwrap(), first);
    let repository_a = AgentRepository::new(db.a.clone());
    let repository_b = AgentRepository::new(db.b.clone());
    let (left, right) = tokio::join!(
        repository_a.mutate_agent_definition(
            &a,
            "writer",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Left")
            }
        ),
        repository_b.mutate_agent_definition(
            &a,
            "writer",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Right")
            }
        )
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        left == Err(AgentManagementError::Conflict) || right == Err(AgentManagementError::Conflict)
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .mutate_agent_definition(&a, "writer", request, None, create)
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_agent_definition(
                &a,
                "another",
                request,
                None,
                AgentDefinitionMutation::Create {
                    name: "Changed".into(),
                    description: "Different payload".into(),
                    content: content("New")
                }
            )
            .await,
        Err(AgentManagementError::Conflict)
    );
    let changes = db
        .committed(veoveo_platform_store::ObservationTable::new(
            veoveo_modules::TableName::new("agent_definition").unwrap(),
            veoveo_platform_store::ObservationReplay::Changefeed(
                veoveo_platform_store::ChangefeedRetention::from_days(30).unwrap(),
            ),
        ))
        .await;
    let revisions = changes
        .iter()
        .map(|row| {
            row["revision"]
                .as_u64()
                .expect("committed definition revision")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(revisions, std::collections::BTreeSet::from([1, 2]));
}

#[tokio::test]
async fn current_context_identity_and_publication_audience_are_fenced() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    let outsider = identity(&db.a, "other-tenant", "outsider").await;
    context(&db.a, &alice, "research").await;
    context(&db.a, &alice, "private").await;
    context(&db.a, &outsider, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let private = authority(&db.a, &alice, "private").await;
    let outside = authority(&db.a, &outsider, "research").await;
    let created = create(&db.a, &a, "assistant").await;
    let mut viewer = a.clone();
    viewer.membership = WorkContextMembershipLevel::Viewer;
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_agent_definition(
                &viewer,
                "assistant",
                Uuid::now_v7(),
                Some(1),
                AgentDefinitionMutation::Draft {
                    content: content("Denied")
                }
            )
            .await,
        Err(AgentManagementError::Forbidden)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_definition(&private, "assistant")
            .await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_definition(&outside, "assistant")
            .await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_agent_definition(
                &a,
                "assistant",
                Uuid::now_v7(),
                Some(1),
                AgentDefinitionMutation::Publish {
                    digest: created.draft_digest.clone(),
                    audience: audience(&outside)
                }
            )
            .await,
        Err(AgentManagementError::Forbidden)
    );
    db.a.client()
        .query(include_str!("queries/agent_management/current_context_identity_and_publication_audience_are_fenced/statement_1.surql"))
        .bind(("context", private.work_context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_agent_definition(
                &a,
                "assistant",
                Uuid::now_v7(),
                Some(1),
                AgentDefinitionMutation::Publish {
                    digest: created.draft_digest.clone(),
                    audience: audience(&private)
                }
            )
            .await,
        Err(AgentManagementError::Forbidden)
    );
    publish(&db.a, &a, &created).await;
    db.a.client()
        .query(include_str!("queries/agent_management/current_context_identity_and_publication_audience_are_fenced/statement_2.surql"))
        .bind(("principal", alice.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_catalog(&a, None, 10)
            .await,
        Err(AgentManagementError::Forbidden)
    );
}

#[tokio::test]
async fn capacity_is_atomic_across_replicas_and_retry_does_not_reserve_twice() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    context(&db.a, &alice, "research").await;
    let mut a = authority(&db.a, &alice, "research").await;
    a.definition_limit = 1;
    let mutation = AgentDefinitionMutation::Create {
        name: "Only one".into(),
        description: "Capacity fixture".into(),
        content: content("Bounded"),
    };
    let repository_a = AgentRepository::new(db.a.clone());
    let repository_b = AgentRepository::new(db.b.clone());
    let (first, second) = tokio::join!(
        repository_a.mutate_agent_definition(&a, "one", Uuid::now_v7(), None, mutation.clone()),
        repository_b.mutate_agent_definition(&a, "two", Uuid::now_v7(), None, mutation)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(
        first == Err(AgentManagementError::Capacity)
            || second == Err(AgentManagementError::Capacity)
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .agent_definitions(&a, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn executable_input_rejects_unknown_secret_and_unbounded_fields() {
    let mut value = serde_json::to_value(content("Safe instructions")).unwrap();
    value["model"]["api_key"] = serde_json::json!("must-not-be-accepted");
    assert!(serde_json::from_value::<AgentContent>(value).is_err());
    let mut invalid = content("Instructions");
    invalid.tools.push(invalid.tools[0].clone());
    assert_eq!(
        invalid.validate(),
        Err(AgentManagementError::Invalid("tools"))
    );
    invalid = content("Instructions");
    invalid.budgets.max_completion_calls = 1000;
    assert_eq!(
        invalid.validate(),
        Err(AgentManagementError::Invalid("budgets"))
    );
    let mut managed = content("Reviewed template");
    managed.execution = AgentExecution::Managed {
        template: "pilot".into(),
        template_revision: "b".repeat(64),
        parameters: std::collections::BTreeMap::from([
            (
                "vehicle".into(),
                AgentTemplateParameter::Text("uav-1".into()),
            ),
            ("enabled".into(), AgentTemplateParameter::Boolean(true)),
            ("count".into(), AgentTemplateParameter::Integer(1)),
        ]),
        resource_subscriptions: vec!["uav-sim://session/test".into()],
    };
    assert!(managed.validate().is_ok());
    use surrealdb::types::SurrealValue;
    assert_eq!(
        AgentContent::from_value(managed.clone().into_value()).unwrap(),
        managed
    );
}

#[tokio::test]
async fn current_view_revisions_ignore_private_writes_and_change_on_removal() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "catalog-revisions", "alice").await;
        let bob = identity(&db.a, "catalog-revisions", "bob").await;
        context(&db.a, &alice, "research").await;
        let a = authority(&db.a, &alice, "research").await;
        let b = authority(&db.b, &bob, "research").await;
        let public_before = AgentRepository::new(db.b.clone())
            .agent_catalog_revision(&b)
            .await
            .unwrap();
        let bob_before = AgentRepository::new(db.b.clone())
            .agent_management_revision(&b)
            .await
            .unwrap();
        let alice_before = AgentRepository::new(db.a.clone())
            .agent_management_revision(&a)
            .await
            .unwrap();
        let private = create(&db.a, &a, "private-draft").await;
        assert_eq!(
            AgentRepository::new(db.b.clone())
                .agent_catalog_revision(&b)
                .await
                .unwrap(),
            public_before
        );
        assert_eq!(
            AgentRepository::new(db.b.clone())
                .agent_management_revision(&b)
                .await
                .unwrap(),
            bob_before
        );
        assert_ne!(
            AgentRepository::new(db.a.clone())
                .agent_management_revision(&a)
                .await
                .unwrap(),
            alice_before
        );
        let published = publish(&db.a, &a, &private).await;
        let visible = AgentRepository::new(db.b.clone())
            .agent_catalog_revision(&b)
            .await
            .unwrap();
        assert_ne!(visible, public_before);
        assert_ne!(
            AgentRepository::new(db.b.clone())
                .agent_management_revision(&b)
                .await
                .unwrap(),
            bob_before
        );
        AgentRepository::new(db.a.clone())
            .mutate_agent_definition(
                &a,
                &published.key,
                Uuid::now_v7(),
                Some(published.revision),
                AgentDefinitionMutation::Status {
                    status: AgentDefinitionStatus::Disabled,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            AgentRepository::new(db.b.clone())
                .agent_catalog_revision(&b)
                .await
                .unwrap(),
            public_before
        );
        assert_eq!(
            AgentRepository::new(db.b.clone())
                .agent_management_revision(&b)
                .await
                .unwrap(),
            bob_before
        );
    })
    .await
    .expect("catalog revision qualification deadline");
}

#[tokio::test]
async fn private_sql_admission_excludes_malformed_hidden_rows_before_decoding() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "malformed-private", "alice").await;
        let bob = identity(&db.a, "malformed-private", "bob").await;
        let foreign = identity(&db.a, "malformed-foreign", "alice").await;
        context(&db.a, &alice, "shared").await;
        context(&db.a, &alice, "other").await;
        context(&db.a, &foreign, "shared").await;
        let admitted = authority(&db.a, &alice, "shared").await;
        let denied = [
            authority(&db.b, &bob, "shared").await,
            authority(&db.b, &alice, "other").await,
            authority(&db.b, &foreign, "shared").await,
        ];
        let definition = create(&db.a, &admitted, "malformed").await;
        db.a.client()
            .query(include_str!("queries/admission/corrupt_draft.surql"))
            .bind(("definition", definition.id))
            .await
            .unwrap()
            .check()
            .unwrap();
        let reader = AgentRepository::new(db.b.clone());
        for caller in denied {
            assert!(
                reader
                    .agent_definitions(&caller, None, 20)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                reader.agent_definition(&caller, "malformed").await,
                Err(AgentManagementError::NotFound)
            );
            assert!(
                reader
                    .agent_catalog(&caller, None, 20)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(
            reader.agent_definition(&admitted, "malformed").await,
            Err(AgentManagementError::Unavailable)
        );
        assert_eq!(
            reader.agent_definitions(&admitted, None, 20).await,
            Err(AgentManagementError::Unavailable)
        );
    })
    .await
    .expect("private SQL admission exceeded 60 seconds");
}

#[tokio::test]
async fn catalog_sql_admission_excludes_malformed_hidden_publications_before_decoding() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "malformed-catalog", "alice").await;
        let bob = identity(&db.a, "malformed-catalog", "bob").await;
        context(&db.a, &alice, "shared").await;
        let editor = authority(&db.a, &alice, "shared").await;
        let caller = authority(&db.b, &bob, "shared").await;
        let definition = publish(&db.a, &editor, &create(&db.a, &editor, "malformed").await).await;
        let reader = AgentRepository::new(db.b.clone());
        assert_eq!(
            reader.agent_catalog(&caller, None, 20).await.unwrap().len(),
            1
        );
        let revision = reader
            .agent_revision(&caller, &definition.key, &definition.draft_digest)
            .await
            .unwrap();
        // A fixture replaces the immutable row rather than weakening READONLY
        // schema fields; malformed model content is admitted only as stored data.
        db.a.client()
            .query(include_str!(
                "queries/admission/corrupt_catalog_revision.surql"
            ))
            .bind(("revision", revision.id.clone()))
            .bind(("content", revision))
            .bind(("definition", definition.id.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            reader
                .agent_catalog(&caller, None, 20)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reader
                .agent_revision(&caller, &definition.key, &definition.draft_digest)
                .await,
            Err(AgentManagementError::NotFound)
        );
        assert_eq!(
            reader
                .agent_executable(&caller, &definition.key, None)
                .await,
            Err(AgentManagementError::NotFound)
        );
        db.a.client()
            .query(include_str!("queries/admission/restore_audience.surql"))
            .bind(("definition", definition.id))
            .bind(("context", editor.work_context))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            reader.agent_catalog(&caller, None, 20).await,
            Err(AgentManagementError::Unavailable)
        );
        assert_eq!(
            reader
                .agent_revision(&caller, &definition.key, &definition.draft_digest)
                .await,
            Err(AgentManagementError::Unavailable)
        );
        assert_eq!(
            reader
                .agent_executable(&caller, &definition.key, None)
                .await,
            Err(AgentManagementError::Unavailable)
        );
    })
    .await
    .expect("catalog SQL admission exceeded 60 seconds");
}

#[tokio::test]
async fn private_draft_unknown_fields_are_rejected_after_sql_admission() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "draft-decoder", "alice").await;
        let bob = identity(&db.a, "draft-decoder", "bob").await;
        context(&db.a, &alice, "operations").await;
        let admitted = authority(&db.a, &alice, "operations").await;
        let denied = authority(&db.a, &bob, "operations").await;
        let definition = create(&db.a, &admitted, "private").await;
        let mut response =
            db.a.client()
                .query(include_str!(
                    "queries/agent_management/closed_records/read.surql"
                ))
                .bind(("record", definition.id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let original: Value = response.take(0).unwrap();
        let repo = AgentRepository::new(db.b.clone());
        for nested in [None, Some("model"), Some("budgets"), Some("execution")] {
            let mut value = original.clone();
            let Value::Object(fields) = &mut value else {
                unreachable!()
            };
            let Value::Object(draft) = fields.get_mut("draft").unwrap() else {
                unreachable!()
            };
            let target = if let Some(key) = nested {
                let Value::Object(target) = draft.get_mut(key).unwrap() else {
                    unreachable!()
                };
                target
            } else {
                draft
            };
            target.insert("undeclared", true.into_value());
            db.a.client()
                .query(include_str!(
                    "queries/agent_management/closed_records/write.surql"
                ))
                .bind(("record", definition.id.clone()))
                .bind(("content", value))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_eq!(
                repo.agent_definition(&denied, "private").await,
                Err(AgentManagementError::NotFound)
            );
            assert!(
                repo.agent_definitions(&denied, None, 20)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                repo.agent_definition(&admitted, "private").await,
                Err(AgentManagementError::Unavailable),
                "{nested:?}"
            );
        }
        db.a.client()
            .query(include_str!(
                "queries/agent_management/closed_records/write.surql"
            ))
            .bind(("record", definition.id.clone()))
            .bind(("content", original))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            repo.agent_definition(&admitted, "private").await.unwrap(),
            definition
        );
    })
    .await
    .expect("private draft decoder qualification deadline");
}
