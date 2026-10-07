use std::collections::BTreeSet;
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_mcp::contract::{KnowledgeResource, SearchRequest};
use veoveo_types::{ResourceAddress, ResourceUri};

#[test]
fn resources_round_trip_and_reject_ambiguous_routes() {
    for uri in [
        "knowledge://sources",
        "knowledge://sources?cursor=map",
        "knowledge://source/map",
        "knowledge://collection/map.features",
        "knowledge://docs",
        "knowledge://docs?cursor=agents",
        "knowledge://docs/design",
        "knowledge://contract",
    ] {
        let uri = ResourceUri::new(uri).unwrap();
        assert_eq!(
            KnowledgeResource::parse(&uri).unwrap().to_uri().unwrap(),
            uri
        );
    }
    for uri in [
        "knowledge://sources/",
        "knowledge://source",
        "knowledge://source/map?cursor=x",
        "knowledge://sources?cursor=map&cursor=time",
        "knowledge://collection/map%2Efeatures",
        "knowledge://docs?extra=value",
        "knowledge://contract/design",
        "other://source/map",
        "knowledge://source/map%2Ftime",
    ] {
        assert!(
            KnowledgeResource::parse(&ResourceUri::new(uri).unwrap()).is_err(),
            "{uri}"
        );
    }
}

#[test]
fn search_bounds_apply_to_deserialization_and_builders() {
    assert!(
        SearchRequest::new(
            EmbeddingText::new("facility").unwrap(),
            BTreeSet::new(),
            BTreeSet::new(),
            21
        )
        .is_err()
    );
    for value in [
        serde_json::json!({"query":"facility","limit":0}),
        serde_json::json!({"query":"facility","tenant":"forged"}),
        serde_json::json!({"query":" "}),
    ] {
        assert!(serde_json::from_value::<SearchRequest>(value).is_err());
    }
    assert_eq!(
        serde_json::from_value::<SearchRequest>(serde_json::json!({"query":"facility"}))
            .unwrap()
            .limit(),
        10
    );
}

#[test]
fn statistics_decode_checks_count_and_timestamp_relationships() {
    use veoveo_knowledge_mcp::contract::CollectionStatistics;
    let empty = serde_json::to_value(CollectionStatistics::empty()).unwrap();
    assert_eq!(
        serde_json::from_value::<CollectionStatistics>(empty.clone()).unwrap(),
        CollectionStatistics::empty()
    );
    for (field, value) in [
        ("indexedMembers", serde_json::json!(1)),
        ("indexedChunks", serde_json::json!(1)),
        ("lastModifiedAt", serde_json::json!("2026-10-01T00:00:00Z")),
        ("forged", serde_json::json!(true)),
    ] {
        let mut invalid = empty.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<CollectionStatistics>(invalid).is_err());
    }
}

#[cfg(feature = "runtime")]
#[test]
fn contract_declaration_exposes_every_compliance_status() {
    use veoveo_mcp_contract::{docs::RequirementId, server_contract::McpServerContract};
    let declaration =
        veoveo_knowledge_mcp::mcp::KnowledgeContract::documents().contract_declaration();
    assert_eq!(declaration.server().as_str(), "knowledge");
    assert_eq!(declaration.contract_revision(), 4);
    assert_eq!(
        declaration
            .compliance()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        <RequirementId as veoveo_types::Vocabulary>::ALL
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
    );
}
