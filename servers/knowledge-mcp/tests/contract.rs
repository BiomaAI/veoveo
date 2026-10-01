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
