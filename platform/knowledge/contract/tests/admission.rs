use veoveo_knowledge_contract::{ChunkSettings, GenerationId};

#[test]
fn generation_and_chunk_settings_reject_unusable_values_on_deserialization() {
    let id = GenerationId::new();
    assert_eq!(
        serde_json::from_str::<GenerationId>(&serde_json::to_string(&id).unwrap()).unwrap(),
        id
    );
    assert!(GenerationId::try_from(uuid::Uuid::nil().to_string()).is_err());
    assert!(GenerationId::try_from("019A5E81-ABCD-7123-8123-123456789ABC".to_owned()).is_err());
    for value in [
        serde_json::json!({"version":"v1", "maxCharacters":100, "overlapCharacters":100}),
        serde_json::json!({"version":"v1", "maxCharacters":0, "overlapCharacters":0}),
        serde_json::json!({"version":"v1", "maxCharacters":100, "overlapCharacters":1, "extra":true}),
    ] {
        assert!(serde_json::from_value::<ChunkSettings>(value).is_err());
    }
}
