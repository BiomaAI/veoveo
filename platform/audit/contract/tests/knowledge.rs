use veoveo_audit_contract::*;
use veoveo_mcp_knowledge_extension::{Observation, Revision, content_digest, docs};
use veoveo_types::{ResourceScheme, ResourceUri, ServerSlug};

fn draft() -> AuditDraft {
    let server = ServerSlug::parse("time").unwrap();
    let uri = ResourceUri::new("time://docs/design").unwrap();
    let collection = docs::collection(&server, &ResourceScheme::parse("time").unwrap());
    let observation = Observation::builder(
        collection.collection().clone(),
        Revision::parse("opaque-revision").unwrap(),
        content_digest("body"),
        chrono::Utc::now(),
    )
    .external(veoveo_mcp_knowledge_extension::ExternalRecord {
        system: "fixture".parse().unwrap(),
        native_id: "record-1".parse().unwrap(),
        url: Some(
            "https://example.test/record?signature=PRIVATE-QUERY-CANARY"
                .parse()
                .unwrap(),
        ),
        mirrored_at: None,
    })
    .build(&collection)
    .unwrap();
    AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Resource {
            server,
            uri: uri.clone(),
        },
        AuditDetail::KnowledgeRead {
            member: uri,
            observation: Some(Box::new((&observation).into())),
            status: KnowledgeReadStatus::Read,
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .build()
    .unwrap()
}
#[test]
fn knowledge_audit_rejects_inconsistent_target_observation_and_outcome() {
    let value = serde_json::to_value(draft()).unwrap();
    assert!(
        AuditTargetRegistry::empty()
            .decoder()
            .from_value::<AuditDraft>(value.clone())
            .is_ok()
    );
    assert!(!value.to_string().contains("PRIVATE-QUERY-CANARY"));
    let mut with_url = value.clone();
    with_url["detail"]["observation"]["external"]["url"] =
        serde_json::json!("https://example.test/");
    assert!(
        AuditTargetRegistry::empty()
            .decoder()
            .from_value::<AuditDraft>(with_url)
            .is_err()
    );
    for (pointer, replacement) in [
        ("/target/server", serde_json::json!("map")),
        ("/target/uri", serde_json::json!("time://docs/agents")),
        ("/detail/status", serde_json::json!("not_modified")),
        ("/detail/observation", serde_json::Value::Null),
        ("/outcome", serde_json::json!("allowed")),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            AuditTargetRegistry::empty()
                .decoder()
                .from_value::<AuditDraft>(invalid)
                .is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn absent_and_null_knowledge_access_match_emitted_bytes_and_schema() {
    let emitted = serde_json::to_value(draft()).unwrap();
    let observation = emitted["detail"]["observation"].clone();
    assert!(!observation.as_object().unwrap().contains_key("access"));
    let schema = schemars::schema_for!(KnowledgeReadObservation);
    let validator = jsonschema::validator_for(&serde_json::to_value(&schema).unwrap()).unwrap();
    let required = schema
        .as_object()
        .unwrap()
        .get("required")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!required.contains(&serde_json::json!("access")));
    for null in [false, true] {
        let mut wire = observation.clone();
        if null {
            wire["access"] = serde_json::Value::Null;
        }
        assert!(validator.is_valid(&wire));
        let input = serde_json::to_vec(&wire).unwrap();
        let decoded: KnowledgeReadObservation = serde_json::from_slice(&input).unwrap();
        assert!(decoded.access.is_none());
        // The producer continues to omit None; receiving explicit null does not
        // change frozen serialized records or introduce a second stored profile.
        assert_eq!(serde_json::to_value(&decoded).unwrap(), observation);
        let mut record = emitted.clone();
        record["detail"]["observation"] = wire;
        assert!(
            AuditTargetRegistry::empty()
                .decoder()
                .from_value::<AuditDraft>(record)
                .is_ok()
        );
    }
    for malformed in [
        serde_json::json!(false),
        serde_json::json!({}),
        serde_json::json!({"readPolicy":{"kind":"unknown"}}),
    ] {
        let mut wire = observation.clone();
        wire["access"] = malformed;
        assert!(!validator.is_valid(&wire));
        assert!(serde_json::from_value::<KnowledgeReadObservation>(wire).is_err());
    }
}

#[test]
fn knowledge_policy_cut_preserves_frozen_audit_access_bytes() {
    use veoveo_mcp_knowledge_extension::{AccessDescriptor, ReadPolicy};
    let policies = [
        (ReadPolicy::Tenant {}, "tenant", "tenant"),
        (ReadPolicy::Subjects {}, "subjects", "subjects"),
        (ReadPolicy::WorkContext {}, "work-context", "work_context"),
        (
            ReadPolicy::SelectedWorkContext {},
            "selected-work-context",
            "selected_work_context",
        ),
        (
            ReadPolicy::SelectedWorkContextMembers {},
            "selected-work-context-members",
            "selected_work_context_members",
        ),
        (
            ReadPolicy::SubjectsInContext {
                profile: Some("operations".parse().unwrap()),
            },
            "subjects-in-context",
            "subjects_in_context",
        ),
    ];
    for (policy, frozen, public) in policies {
        let access = AccessDescriptor {
            tenant: "tenant".parse().unwrap(),
            work_context: "mission".parse().unwrap(),
            read_policy: policy,
            owner: veoveo_types::AccessSubject::Principal("pilot".parse().unwrap()),
            grants: vec![],
            data_labels: vec![],
            expires_at: None,
        };
        assert_eq!(
            serde_json::to_value(&access).unwrap()["readPolicy"]["kind"],
            public
        );
        let mut observed = serde_json::to_value(draft()).unwrap()["detail"]["observation"].clone();
        let mut frozen_access = serde_json::to_value(&access).unwrap();
        frozen_access["readPolicy"]["kind"] = serde_json::json!(frozen);
        observed["access"] = frozen_access;
        let decoded: KnowledgeReadObservation = serde_json::from_value(observed.clone()).unwrap();
        assert_eq!(decoded.access.as_ref(), Some(&access));
        assert_eq!(serde_json::to_value(&decoded).unwrap(), observed);
        let bytes = serde_json::to_vec(&decoded).unwrap();
        let again: KnowledgeReadObservation = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&again).unwrap(), bytes);
        let schema = schemars::schema_for!(KnowledgeReadObservation);
        let validator = jsonschema::validator_for(&serde_json::to_value(schema).unwrap()).unwrap();
        assert!(validator.is_valid(&observed));
        for extra in ["undeclared", "read_policy"] {
            let mut bad = observed.clone();
            bad["access"][extra] = serde_json::json!(true);
            assert!(!validator.is_valid(&bad));
            assert!(serde_json::from_value::<KnowledgeReadObservation>(bad).is_err());
        }
        let mut bad = observed.clone();
        bad["access"]["readPolicy"]["undeclared"] = serde_json::json!(true);
        assert!(!validator.is_valid(&bad));
        assert!(serde_json::from_value::<KnowledgeReadObservation>(bad).is_err());
        if frozen != public {
            observed["access"]["readPolicy"]["kind"] = serde_json::json!(public);
            assert!(serde_json::from_value::<KnowledgeReadObservation>(observed).is_err());
        }
    }
}
