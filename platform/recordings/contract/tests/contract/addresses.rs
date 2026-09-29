use serde_json::{Value, json};
use veoveo_recording_contract::{uris, *};
use veoveo_types::{ResourceAddress, ResourceTemplateUri};

const ID: &str = "01983da0-0000-7000-8000-000000000001";
const URI: &str = "recording://recordings/01983da0-0000-7000-8000-000000000001";

fn id() -> RecordingId {
    ID.parse().unwrap()
}

fn cursor() -> RecordingCatalogCursor {
    RecordingCatalogCursor::new("2026-09-28T12:00:00Z".parse().unwrap(), id())
}

#[test]
fn identities_require_canonical_rfc_uuid_v7_at_wire_admission() {
    assert_eq!(RecordingId::try_from(id().as_uuid()).unwrap(), id());
    assert_eq!(serde_json::to_value(id()).unwrap(), ID);
    assert_eq!(
        serde_json::from_value::<RecordingId>(json!(ID)).unwrap(),
        id()
    );
    assert_eq!(RecordingId::new().as_uuid().get_version_num(), 7);
    for invalid in [
        "private-recording",
        "00000000-0000-0000-0000-000000000000",
        "01983da0-0000-4000-8000-000000000001",
        "01983da0-0000-7000-0000-000000000001",
        "01983da0-0000-7000-c000-000000000001",
        "01983DA0-0000-7000-8000-000000000001",
        "01983da0000070008000000000000001",
        "urn:uuid:01983da0-0000-7000-8000-000000000001",
    ] {
        let error = RecordingId::parse(invalid).unwrap_err();
        assert_eq!(error, RecordingContractError::Identity);
        assert!(!error.to_string().contains(invalid));
        assert!(serde_json::from_value::<RecordingId>(json!(invalid)).is_err());
        assert!(
            serde_json::from_value::<SealRecordingRequest>(json!({"recording_id":invalid}))
                .is_err()
        );
    }
}

#[test]
fn every_recording_identity_uses_the_same_admission_profile() {
    macro_rules! qualify {
        ($id:ty) => {
            let id = <$id>::parse(ID).unwrap();
            assert_eq!(<$id>::try_from(id.as_uuid()).unwrap(), id);
            assert_eq!(serde_json::to_value(id).unwrap(), ID);
            assert_eq!(serde_json::from_value::<$id>(json!(ID)).unwrap(), id);
            assert_eq!(<$id>::new().as_uuid().get_version_num(), 7);
            assert_eq!(
                serde_json::to_value(schemars::schema_for!($id)).unwrap()["type"],
                "string"
            );
            for invalid in [
                "private-identity",
                "00000000-0000-0000-0000-000000000000",
                "01983da0-0000-4000-8000-000000000001",
                "01983da0-0000-7000-c000-000000000001",
                "01983DA0-0000-7000-8000-000000000001",
                "01983da0000070008000000000000001",
                "urn:uuid:01983da0-0000-7000-8000-000000000001",
            ] {
                let error = <$id>::parse(invalid).unwrap_err();
                assert!(!error.to_string().contains(invalid));
                assert!(serde_json::from_value::<$id>(json!(invalid)).is_err());
            }
        };
    }
    qualify!(RecordingId);
    qualify!(RecordingDatasetId);
    qualify!(RecordingLayerId);
    qualify!(RecordingReadGrantId);
    qualify!(RecordingProjectionId);
}

#[test]
fn catalog_selection_is_bounded_and_canonical_at_construction_and_decoding() {
    let dataset = RecordingDatasetId::new();
    let a = RecordingId::parse(ID).unwrap();
    let b = RecordingId::parse("01983da0-0000-7000-8000-000000000002").unwrap();
    let request = CreateRecordingCatalogGrantRequest::new(dataset, vec![b, a, b]).unwrap();
    assert_eq!(request.dataset_id(), dataset);
    assert_eq!(request.recording_ids(), &[a, b]);
    let decoded: CreateRecordingCatalogGrantRequest =
        serde_json::from_value(json!({"dataset_id":dataset, "recording_ids":[b,a,b]})).unwrap();
    assert_eq!(decoded.recording_ids(), &[a, b]);
    for count in [0, 501] {
        assert!(CreateRecordingCatalogGrantRequest::new(dataset, vec![a; count]).is_err());
        assert!(
            serde_json::from_value::<CreateRecordingCatalogGrantRequest>(
                json!({"dataset_id":dataset,"recording_ids":vec![a;count]})
            )
            .is_err()
        );
    }
    assert!(CreateRecordingCatalogGrantRequest::new(dataset, vec![a; 500]).is_ok());
    for value in [
        json!({"dataset_id":"01983da0-0000-4000-8000-000000000001","recording_ids":[a]}),
        json!({"dataset_id":dataset,"recording_ids":["01983da0-0000-7000-c000-000000000001"]}),
    ] {
        assert!(serde_json::from_value::<CreateRecordingCatalogGrantRequest>(value).is_err());
    }
}

#[test]
fn every_resource_family_round_trips_through_the_public_contract() {
    for resource in [
        RecordingResource::Docs,
        RecordingResource::Document(RecordingDocument::Agents),
        RecordingResource::Document(RecordingDocument::Design),
        RecordingResource::Contract,
        RecordingResource::Explorer,
        RecordingResource::Catalog(None),
        RecordingResource::Catalog(Some(cursor())),
        RecordingResource::Recording(RecordingUri::new(id())),
        RecordingResource::Layers(RecordingLayersUri::new(id())),
    ] {
        let uri = resource.to_uri().unwrap();
        assert_eq!(RecordingResource::parse(uri.as_str()).unwrap(), resource);
        assert_eq!(
            <RecordingResource as ResourceAddress>::parse(&uri).unwrap(),
            resource
        );
        let wire = serde_json::to_value(&resource).unwrap();
        assert_eq!(wire, uri.as_str());
        assert_eq!(
            serde_json::from_value::<RecordingResource>(wire).unwrap(),
            resource
        );
    }
    assert_eq!(RecordingUri::new(id()).as_str(), URI);
    assert_eq!(RecordingUri::parse(URI).unwrap().id(), id());
    assert_eq!(RecordingLayersUri::new(id()).id(), id());
    assert!(RecordingUri::parse(RecordingLayersUri::new(id()).as_str()).is_err());
    assert!(RecordingLayersUri::parse(URI).is_err());
}

#[test]
fn resource_admission_rejects_wrong_parents_queries_and_alternate_spellings() {
    for invalid in [
        "recording://recording/01983da0-0000-7000-8000-000000000001",
        "recording://recordings/01983da0-0000-7000-8000-000000000001/extra",
        "recording://recordings/01983da0-0000-7000-8000-000000000001/",
        "recording://recordings/%301983da0-0000-7000-8000-000000000001",
        "recording://recordings/01983da0-0000-7000-8000-000000000001%2Flayers",
        "recording://recordings/01983da0-0000-7000-8000-000000000001?cursor=00",
        "recording://recordings/01983da0-0000-7000-8000-000000000001#secret",
        "recording://recordings//01983da0-0000-7000-8000-000000000001",
        "recording://secret@catalog",
        "recording://catalog:80",
        "recording://catalog/",
        "recording://catalog?",
        "recording://catalog?cursor=",
        "recording://catalog?offset=1",
        "recording://catalog?cursor=00&cursor=00",
        "recording://docs/other",
        "recording://docs/agents/extra",
        "recording://docs?cursor=00",
        "RECORDING://catalog",
        "ui://recording/other.html",
    ] {
        assert!(
            RecordingResource::parse(invalid).is_err(),
            "accepted {invalid}"
        );
        assert!(serde_json::from_value::<RecordingResource>(json!(invalid)).is_err());
    }
    for suffix in ["&cursor=00", "&offset=1", "#fragment"] {
        let valid = RecordingResource::Catalog(Some(cursor())).to_uri().unwrap();
        assert!(RecordingResource::parse(format!("{valid}{suffix}")).is_err());
    }
}

#[test]
fn cursor_admission_checks_collection_version_position_and_encoding() {
    let valid = cursor();
    assert_eq!(
        RecordingCatalogCursor::parse(valid.as_str()).unwrap(),
        valid
    );
    assert_eq!(valid.recording_id(), id());
    assert_eq!(
        valid.started_at(),
        "2026-09-28T12:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
    );
    let wire = serde_json::to_value(&valid).unwrap();
    assert_eq!(wire, valid.as_str());
    assert_eq!(
        serde_json::from_value::<RecordingCatalogCursor>(wire).unwrap(),
        valid
    );
    let bytes = hex::decode(valid.as_str()).unwrap();
    let envelope: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        envelope,
        json!({"version":1,"collection":"recording://catalog",
        "position":{"started_at":"2026-09-28T12:00:00Z","recording_id":ID}})
    );
    for (pointer, value) in [
        ("/version", json!(2)),
        ("/collection", json!("time://events")),
        (
            "/position/recording_id",
            json!("01983da0-0000-4000-8000-000000000001"),
        ),
        ("/position/started_at", json!("private-timestamp")),
        ("/position", json!({})),
    ] {
        let mut invalid = envelope.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            RecordingCatalogCursor::parse(hex::encode(serde_json::to_vec(&invalid).unwrap()))
                .is_err()
        );
    }
    for pointer in ["", "/position"] {
        let mut invalid = envelope.clone();
        invalid.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        assert!(
            RecordingCatalogCursor::parse(hex::encode(serde_json::to_vec(&invalid).unwrap()))
                .is_err()
        );
    }
    for invalid in [
        String::new(),
        "private-cursor".into(),
        "00".into(),
        "a".repeat(2049),
        valid.as_str().to_uppercase(),
        hex::encode([bytes.as_slice(), b" "].concat()),
    ] {
        let error = RecordingCatalogCursor::parse(invalid).unwrap_err();
        assert_eq!(error.to_string(), "invalid Recording catalog cursor");
    }
}

#[test]
fn discovery_templates_expand_to_the_typed_builder_addresses() {
    let cursor = cursor();
    for (template, key, value, expected) in [
        (
            uris::DOC_TEMPLATE,
            "doc_id",
            "design",
            uris::doc_uri(RecordingDocument::Design).to_string(),
        ),
        (
            uris::RECORDING_TEMPLATE,
            "recording_id",
            ID,
            uris::recording_uri(id()).to_string(),
        ),
        (
            uris::LAYERS_TEMPLATE,
            "recording_id",
            ID,
            uris::layers_uri(id()).to_string(),
        ),
        (
            uris::CATALOG_TEMPLATE,
            "cursor",
            cursor.as_str(),
            RecordingResource::Catalog(Some(cursor.clone())).to_string(),
        ),
    ] {
        let expanded = ResourceTemplateUri::new(template)
            .unwrap()
            .expand_scalars(&[(key.to_string(), value.to_string())].into())
            .unwrap();
        assert_eq!(expanded.as_str(), expected);
        RecordingResource::parse(expanded.as_str()).unwrap();
    }
    assert_eq!(
        ResourceTemplateUri::new(uris::CATALOG_TEMPLATE)
            .unwrap()
            .expand_scalars(&Default::default())
            .unwrap()
            .as_str(),
        uris::CATALOG_URI
    );
}

#[test]
fn public_identity_address_and_cursor_schemas_keep_the_string_wire_shape() {
    for schema in [
        schemars::schema_for!(RecordingId),
        schemars::schema_for!(RecordingUri),
        schemars::schema_for!(RecordingLayersUri),
        schemars::schema_for!(RecordingResource),
        schemars::schema_for!(RecordingCatalogCursor),
    ] {
        assert_eq!(serde_json::to_value(schema).unwrap()["type"], "string");
    }
    let request: SealRecordingRequest = serde_json::from_value(json!({"recording_id":ID})).unwrap();
    assert_eq!(request.recording_id, id());
    let page = RecordingCatalogPage {
        items: Vec::new(),
        limit: 100,
        next_cursor: Some(cursor()),
    };
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({"items":[],"limit":100,"next_cursor":cursor().as_str()})
    );
}
