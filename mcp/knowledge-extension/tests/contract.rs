use chrono::Utc;
use serde_json::json;
use veoveo_mcp_knowledge_extension::*;
use veoveo_types::{AccessSubject, ResourceScheme, ServerSlug};

fn docs_collection() -> CollectionDescriptor {
    docs::collection(
        &ServerSlug::new("independent").unwrap(),
        &ResourceScheme::new("example").unwrap(),
    )
}

#[test]
fn declarations_reject_unknown_fields_and_inconsistent_freshness() {
    let descriptor = docs_collection();
    let wire = serde_json::to_value(&descriptor).unwrap();
    assert_eq!(wire["collection"], "independent.docs");
    assert_eq!(wire["freshness"], json!({"immutable": true}));
    let schema = schemars::schema_for!(CollectionDescriptor);
    jsonschema::validate(&serde_json::to_value(schema).unwrap(), &wire).unwrap();
    for field in ["typo", "owner"] {
        let mut bad = wire.clone();
        bad[field] = json!("untrusted");
        assert!(serde_json::from_value::<CollectionDescriptor>(bad).is_err());
    }
    for bad in [
        json!({"immutable": false}),
        json!({"immutable": true, "maxAgeSeconds": 30}),
        json!({"maxAgeSeconds": -1}),
        json!({"maxAgeSeconds": 30, "typo": true}),
    ] {
        assert!(serde_json::from_value::<Freshness>(bad).is_err());
    }
    let mut bad = wire;
    bad["changeSignal"] = json!("revalidate");
    assert!(serde_json::from_value::<CollectionDescriptor>(bad).is_err());
    for bad in [
        "docs",
        "server.docs.extra",
        "server./docs",
        "server.",
        "SERVER.docs",
    ] {
        assert!(bad.parse::<CollectionId>().is_err());
    }
}

#[test]
fn access_descriptor_is_required_by_collection_and_cannot_cross_owners() {
    let descriptor = CollectionDescriptor::new(
        "example.records".parse().unwrap(),
        "record".parse().unwrap(),
        veoveo_types::ResourceTemplateUri::new("example://records{?cursor}").unwrap(),
        Freshness::max_age(30),
        ChangeSignal::Listen,
        AccessModel::WorkContext,
        IndexingMode::Content,
    )
    .unwrap();
    let builder = || {
        Observation::builder(
            descriptor.collection().clone(),
            "1".parse().unwrap(),
            content_digest("body"),
            Utc::now(),
        )
    };
    assert!(builder().build(&descriptor).is_err());
    let observed = builder()
        .access(AccessDescriptor {
            read_policy: veoveo_mcp_knowledge_extension::ReadPolicy::WorkContext {},
            tenant: "tenant".parse().unwrap(),
            work_context: "work".parse().unwrap(),
            owner: AccessSubject::Principal("owner".parse().unwrap()),
            grants: vec![],
            data_labels: vec![],
        })
        .build(&descriptor)
        .unwrap();
    assert!(observed.validate_collection(&docs_collection()).is_err());
    let wire = serde_json::to_value(&observed).unwrap();
    assert_eq!(wire["contentSha256"].as_str().unwrap().len(), 64);
    assert!(!wire["contentSha256"].as_str().unwrap().contains(':'));
    let schema = serde_json::to_value(schemars::schema_for!(Observation)).unwrap();
    jsonschema::validate(&schema, &wire).unwrap();
    let mut malformed = wire;
    malformed["contentSha256"] = json!("sha256:wrong");
    assert!(jsonschema::validate(&schema, &malformed).is_err());
    assert!(serde_json::from_value::<Observation>(malformed).is_err());
}

#[test]
fn read_policy_requires_explicit_closed_source_semantics() {
    let schema = serde_json::to_value(schemars::schema_for!(ReadPolicy)).unwrap();
    for wire in [
        json!({"kind": "tenant"}),
        json!({"kind": "subjects"}),
        json!({"kind": "work-context"}),
        json!({"kind": "subjects-in-context"}),
        json!({"kind": "subjects-in-context", "profile": "operations"}),
    ] {
        let policy: ReadPolicy = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(policy).unwrap(), wire);
        jsonschema::validate(&schema, &wire).unwrap();
    }
    for wire in [
        json!({"kind": "unknown"}),
        json!({"kind": "subjects", "profile": "operations"}),
        json!({"kind": "subjects-in-context", "caller": "author"}),
    ] {
        assert!(serde_json::from_value::<ReadPolicy>(wire.clone()).is_err());
        assert!(jsonschema::validate(&schema, &wire).is_err());
    }
    assert!(serde_json::from_value::<AccessDescriptor>(json!({
        "tenant": "tenant", "workContext": "work", "owner": {"kind": "principal", "id": "owner"},
        "dataLabels": []
    })).is_err());
    for profile in ["Operations", "operations/read", "operations read", ""] {
        assert!(
            serde_json::from_value::<ReadPolicy>(json!({
                "kind": "subjects-in-context", "profile": profile,
            }))
            .is_err()
        );
    }
}

#[test]
fn doc_pages_are_sorted_bounded_and_reject_foreign_cursors() {
    let scheme = ResourceScheme::new("example").unwrap();
    let entries: Vec<_> = (0..70)
        .rev()
        .map(|n| {
            let id = DocumentId::new(format!("doc-{n:03}")).unwrap();
            docs::DocumentEntry {
                uri: docs::member_uri(&scheme, &id),
                id,
                title: "Document".into(),
            }
        })
        .collect();
    let first = docs::page(entries.clone(), None).unwrap();
    assert_eq!(first.items.len(), 32);
    let second = docs::page(entries.clone(), first.next_cursor.as_ref()).unwrap();
    assert!(first.items.last().unwrap().id < second.items[0].id);
    let third = docs::page(entries.clone(), second.next_cursor.as_ref()).unwrap();
    assert_eq!(third.items.len(), 6);
    assert!(third.next_cursor.is_none());
    assert!(docs::page(entries, Some(&DocumentId::new("foreign").unwrap())).is_err());
}

#[test]
fn search_bounds_are_unicode_characters_and_scores_are_finite() {
    let uri = || veoveo_types::ResourceUri::new("example://records/one").unwrap();
    assert!(SearchHit::new(uri(), None, Some("é".repeat(320)), Some(1.0)).is_ok());
    assert!(SearchHit::new(uri(), None, Some("é".repeat(321)), None).is_err());
    assert!(SearchHit::new(uri(), None, None, Some(f64::NAN)).is_err());
    assert!(SearchDeclaration::new(vec![]).is_err());
    assert!(SearchDeclaration::new(vec!["example.docs".parse().unwrap(); 2]).is_err());
    let hit = SearchHit::new(uri(), Some("Title".into()), Some("Excerpt".into()), None).unwrap();
    assert!(SearchResults::new(vec![hit.clone(), hit.clone()]).is_err());
    assert!(serde_json::from_value::<SearchResults>(json!({"results": [hit, hit]})).is_err());
    assert!(
        serde_json::from_value::<SearchResults>(json!({"results": [], "hidden": "content"}))
            .is_err()
    );
    let too_many = (0..101)
        .map(|id| {
            SearchHit::new(
                veoveo_types::ResourceUri::new(format!("example://records/{id}")).unwrap(),
                None,
                None,
                None,
            )
            .unwrap()
        })
        .collect();
    assert!(SearchResults::new(too_many).is_err());
    assert_eq!(SearchResults::new(vec![]).unwrap().results().len(), 0);
}

#[test]
fn compiled_document_digest_matches_exact_embedded_bytes() {
    const EMBEDDED: (&str, [u8; 32]) = veoveo_knowledge_macros::embedded_document!("DESIGN.md");
    assert_eq!(
        veoveo_types::Sha256Digest::from_bytes(EMBEDDED.1),
        content_digest(EMBEDDED.0)
    );
    assert_eq!(EMBEDDED.0, include_str!("../DESIGN.md"));
}

#[cfg(feature = "mcp")]
#[test]
fn negotiated_reads_validate_content_and_conditionals_preserve_other_capabilities() {
    use rmcp::model::{ClientCapabilities, RequestMetaObject};
    let descriptor = docs_collection();
    let uri = veoveo_types::ResourceUri::new("example://docs/design").unwrap();
    let text = "# Design\nExact UTF-8: é\n";
    let observation = docs::observation(&descriptor, content_digest(text), Utc::now());
    let mut meta = RequestMetaObject::default();
    let mut capabilities = ClientCapabilities::default();
    capabilities
        .extensions
        .get_or_insert_default()
        .insert("example/other".into(), Default::default());
    meta.set_client_capabilities(capabilities);
    let plain = server::member_result(
        &uri,
        "text/markdown",
        text.into(),
        observation.clone(),
        &descriptor,
        Some(&meta),
    )
    .unwrap();
    assert!(client::observation(&plain).unwrap().is_none());
    client::declare_read(&mut meta, None);
    assert!(
        meta.client_capabilities()
            .unwrap()
            .extensions
            .unwrap()
            .contains_key("example/other")
    );
    let full = server::member_result(
        &uri,
        "text/markdown",
        text.into(),
        observation.clone(),
        &descriptor,
        Some(&meta),
    )
    .unwrap();
    assert_eq!(
        client::validate_read(&full, &uri, None).unwrap(),
        Some(observation.clone())
    );
    let other = veoveo_types::ResourceUri::new("example://docs/agents").unwrap();
    assert!(client::validate_read(&full, &other, None).is_err());
    assert!(
        server::member_result(
            &uri,
            "text/markdown",
            "different".into(),
            observation.clone(),
            &descriptor,
            Some(&meta)
        )
        .is_err()
    );
    client::declare_read(&mut meta, Some(observation.revision()));
    let conditional = server::member_result(
        &uri,
        "text/markdown",
        text.into(),
        observation.clone(),
        &descriptor,
        Some(&meta),
    )
    .unwrap();
    assert!(conditional.contents.is_empty());
    assert!(
        client::validate_read(&conditional, &uri, Some(observation.revision()))
            .unwrap()
            .unwrap()
            .not_modified()
    );
    assert!(client::validate_read(&conditional, &uri, None).is_err());
    client::declare_read(&mut meta, Some(&"another".parse().unwrap()));
    assert_eq!(
        server::member_result(
            &uri,
            "text/markdown",
            text.into(),
            observation,
            &descriptor,
            Some(&meta)
        )
        .unwrap()
        .contents
        .len(),
        1
    );
}
