use veoveo_audit_contract::*;
use veoveo_mcp_knowledge_extension::{Observation, Revision, content_digest, docs};
use veoveo_types::{ResourceScheme, ResourceUri, ServerSlug};

fn draft() -> AuditDraft {
    let server = ServerSlug::new("time").unwrap();
    let uri = ResourceUri::new("time://docs/design").unwrap();
    let collection = docs::collection(&server, &ResourceScheme::new("time").unwrap());
    let observation = Observation::builder(
        collection.collection().clone(),
        Revision::new("opaque-revision").unwrap(),
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
    assert!(serde_json::from_value::<AuditDraft>(value.clone()).is_ok());
    assert!(!value.to_string().contains("PRIVATE-QUERY-CANARY"));
    let mut with_url = value.clone();
    with_url["detail"]["observation"]["external"]["url"] =
        serde_json::json!("https://example.test/");
    assert!(serde_json::from_value::<AuditDraft>(with_url).is_err());
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
            serde_json::from_value::<AuditDraft>(invalid).is_err(),
            "{pointer}"
        );
    }
}
