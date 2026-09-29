use serde_json::json;
use veoveo_timeseries_mcp::contract::TimeseriesForecastRequest;

#[test]
fn forecasting_consumes_source_admission_from_the_owner() {
    let request = |source| {
        json!({
            "source":source,"mapping":{"value_column":"value"},
        "horizon":4,"method":"naive_trend"
        })
    };
    let good = request(
        json!({"kind":"uri","uri":"https://data.example.test/input.csv?part=2&part=1","format":"csv"}),
    );
    let parsed: TimeseriesForecastRequest = serde_json::from_value(good).unwrap();
    let wire = serde_json::to_value(parsed).unwrap();
    assert_eq!(
        wire["source"]["uri"],
        "https://data.example.test/input.csv?part=2&part=1"
    );
    for source in [
        json!({"kind":"uri","uri":"http://data.example.test/input.csv","format":"csv"}),
        json!({"kind":"uris","uris":[],"format":"csv"}),
        json!({"kind":"uri","uri":"https://user:password@data.example.test/input.csv","format":"csv"}),
        json!({"kind":"inline_csv","csv":"value\n1\n","options":{"extra":{"HEADER":true}}}),
        json!({"kind":"inline_csv","csv":"value\n1\n","options":{"extra":{"nullstr":["NA",null]}}}),
    ] {
        assert!(serde_json::from_value::<TimeseriesForecastRequest>(request(source)).is_err());
    }
}

#[test]
fn forecast_schema_matches_the_owning_source_types() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/source-contract.schema.json")).unwrap();
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(TimeseriesForecastRequest)).unwrap(),
        expected
    );
}
