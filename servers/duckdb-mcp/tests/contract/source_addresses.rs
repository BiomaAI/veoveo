use serde_json::json;
use veoveo_artifact_contract::ArtifactId;
use veoveo_duckdb_mcp::contract::*;
use veoveo_types::HttpsUrl;

#[test]
fn source_profiles_agree_on_supported_inputs_without_advertising_artifact_reads_to_every_consumer()
{
    let schema = serde_json::to_value(schemars::schema_for!(DuckDbSource)).unwrap();
    let schema = jsonschema::validator_for(&schema).unwrap();
    let tabular_schema = serde_json::to_value(schemars::schema_for!(DuckDbTabularSource)).unwrap();
    let tabular_schema = jsonschema::validator_for(&tabular_schema).unwrap();
    for source in [
        json!({"kind":"inline_csv","csv":"value\n1\n"}),
        json!({"kind":"uri","uri":"https://data.example.test/a.csv","format":"csv"}),
        json!({"kind":"uris","uris":["https://data.example.test/a.csv"],"format":"csv"}),
        json!({"kind":"artifact","uri":ArtifactId::new().plane_uri(),"format":"parquet"}),
    ] {
        assert!(schema.is_valid(&source));
        let full = serde_json::from_value::<DuckDbSource>(source.clone()).unwrap();
        let is_tabular = source["kind"] != "artifact";
        assert_eq!(tabular_schema.is_valid(&source), is_tabular);
        assert_eq!(
            serde_json::from_value::<DuckDbTabularSource>(source.clone()).is_ok(),
            is_tabular
        );
        let wire = serde_json::to_value(full).unwrap();
        assert_eq!(wire["kind"], source["kind"]);
        if is_tabular {
            let tabular = serde_json::from_value::<DuckDbTabularSource>(source).unwrap();
            assert_eq!(
                serde_json::to_value(DuckDbSource::from(tabular)).unwrap(),
                wire
            );
        }
    }
    for source in [
        json!({"kind":"artifact","uri":"https://data.example.test/a.csv","format":"csv"}),
        json!({"kind":"artifact","uri":ArtifactId::new().plane_uri(),"format":"csv","csv":"value\n1\n"}),
        json!({"kind":"inline_csv","csv":"value\n1\n","uri":ArtifactId::new().plane_uri()}),
        json!({"kind":"uri","uri":"http://data.example.test/a.csv","format":"csv"}),
        json!({"kind":"uris","uris":[],"format":"csv"}),
        json!({"kind":"unknown","csv":"value\n1\n"}),
    ] {
        assert!(!schema.is_valid(&source), "schema accepted {source}");
        assert!(
            serde_json::from_value::<DuckDbSource>(source.clone()).is_err(),
            "decoder accepted {source}"
        );
        assert!(!tabular_schema.is_valid(&source));
        assert!(serde_json::from_value::<DuckDbTabularSource>(source).is_err());
    }
}

#[test]
fn sources_keep_typed_https_urls_and_nonempty_lists_through_decoding() {
    let first =
        HttpsUrl::parse("https://data.example.test:8443/a.csv?part=2&part=1&sig=a%2Fb").unwrap();
    let list = DuckDbSourceUris::new(
        first.clone(),
        ["https://data.example.test/b.csv".parse().unwrap()],
    );
    let source = DuckDbSource::Tabular(DuckDbTabularSource::Uris {
        uris: list.clone(),
        format: DuckDbFormat::Csv,
        options: DuckDbReadOptions::default(),
    });
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["uris"][0], first.as_str());
    assert_eq!(list.as_slice().len(), 2);
    assert_eq!(
        serde_json::from_value::<DuckDbSource>(wire).unwrap(),
        source
    );
    assert!(DuckDbSourceUris::try_from(vec![]).is_err());
    assert!(
        serde_json::from_value::<DuckDbSource>(json!({"kind":"uris","uris":[],"format":"csv"}))
            .is_err()
    );
}

#[test]
fn sources_reject_invalid_urls_and_unknown_fields_before_materialization() {
    for uri in [
        "http://example.test/a.csv",
        "file:///a.csv",
        "https://user:secret@example.test/a.csv",
        "https://example.test/a.csv#part",
        "https://example.test/a.csv?token=%ZZ",
    ] {
        assert!(
            serde_json::from_value::<DuckDbSource>(json!({"kind":"uri","uri":uri,"format":"csv"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<DuckDbSource>(
                json!({"kind":"uris","uris":["https://example.test/b.csv",uri],"format":"csv"})
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<DuckDbSource>(json!({"kind":"uri","uri":"https://example.test/a.csv","format":"csv","filename":"../other"})).is_err());
}

#[test]
fn artifact_sources_admit_only_the_planes_native_occurrence_identity() {
    let id = ArtifactId::new();
    let uri = DuckDbArtifactSourceUri::new(id);
    assert_eq!(uri.as_artifact_uri().artifact_id(), id);
    let source = DuckDbSource::Artifact {
        uri: uri.clone(),
        format: DuckDbFormat::Parquet,
        options: DuckDbReadOptions::default(),
    };
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["uri"], id.plane_uri().as_str());
    assert_eq!(
        serde_json::from_value::<DuckDbSource>(wire).unwrap(),
        source
    );
    for invalid in [
        format!("duckdb://artifact/{id}"),
        format!("artifact://{id}?token=secret"),
        "artifact://01983da0-0000-4000-8000-000000000001".into(),
        "artifact://01983da0-0000-7000-0000-000000000001".into(),
    ] {
        let error = DuckDbArtifactSourceUri::parse(&invalid)
            .unwrap_err()
            .to_string();
        assert!(!error.contains(&invalid));
        assert!(!error.contains("secret"));
        assert!(
            serde_json::from_value::<DuckDbSource>(
                json!({"kind":"artifact","uri":invalid,"format":"parquet"})
            )
            .is_err()
        );
    }
}
