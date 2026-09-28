use veoveo_media_mcp::contract::{GenerationPredictionSummary, GenerationRunOutput};
#[test]
fn generation_schemas_preserve_the_published_profile() {
    let actual = serde_json::json!({"summary":schemars::schema_for!(GenerationPredictionSummary),"output":schemars::schema_for!(GenerationRunOutput)});
    assert_eq!(
        actual,
        serde_json::from_str::<serde_json::Value>(include_str!("fixtures/generation-schemas.json"))
            .unwrap()
    );
}
