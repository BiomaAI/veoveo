use serde_json::json;
use veoveo_recording_contract::{
    PlaybackArchiveUri, RecordingCatalogGrant, RecordingCatalogGrantBuilder,
    RecordingCatalogGrantSchema, RecordingCatalogUri, RecordingDatasetId, RecordingId,
    RecordingReadGrantId, RecordingRedapOrigin,
};

fn dataset() -> RecordingDatasetId {
    "019abcde-abcd-7abc-8abc-abcdefabcdef".parse().unwrap()
}

#[test]
fn loopback_default_ports_fail_with_an_actionable_configuration_error() {
    for (http, redap) in [("http", "rerun+http"), ("https", "rerun")] {
        let port = if http == "http" { 80 } else { 443 };
        for host in ["localhost", "127.0.0.1", "127.0.0.2", "[::1]"] {
            for suffix in [String::new(), format!(":{port}")] {
                let error = RecordingRedapOrigin::from_http(&format!("{http}://{host}{suffix}"))
                    .unwrap_err();
                assert_eq!(
                    error,
                    veoveo_recording_contract::RecordingContractError::RedapLoopbackPort
                );
                assert!(
                    error
                        .to_string()
                        .contains("configure an explicit nondefault port or a public host")
                );
            }
            let entry = format!("{redap}://{host}:{port}/entry/019ABCDEABCD7ABC8abcabcdefabcdef");
            assert!(RecordingCatalogUri::parse(&entry).is_err());
            let archive = format!(
                "{redap}://{host}:{port}/dataset/019ABCDEABCD7ABC8abcabcdefabcdef?segment_id={}",
                RecordingId::new()
            );
            assert!(PlaybackArchiveUri::parse(&archive).is_err());
            assert!(RecordingRedapOrigin::from_http(&format!("{http}://{host}:8443")).is_ok());
        }
        assert!(RecordingRedapOrigin::from_http(&format!("{http}://example.com:{port}")).is_ok());
    }
}

#[test]
fn redap_builders_preserve_network_components_and_domain_identities() {
    let dataset = dataset();
    let recording = RecordingId::new();
    for (http, expected) in [
        ("https://example.com", "rerun://example.com:443"),
        ("http://localhost:8080", "rerun+http://localhost:8080"),
        ("http://127.0.0.1:8080", "rerun+http://127.0.0.1:8080"),
        ("https://[2001:db8::1]:8443", "rerun://[2001:db8::1]:8443"),
        (
            "https://bücher.example",
            "rerun://xn--bcher-kva.example:443",
        ),
    ] {
        let origin = RecordingRedapOrigin::from_http(http).unwrap();
        assert_eq!(origin.as_str(), expected);
        let entry = RecordingCatalogUri::new(&origin, dataset);
        assert_eq!(
            entry.as_str(),
            format!("{expected}/entry/019ABCDEABCD7ABC8abcabcdefabcdef")
        );
        assert_eq!(entry.dataset_id(), dataset);
        assert_eq!(RecordingCatalogUri::parse(entry.as_str()).unwrap(), entry);
        assert_eq!(
            serde_json::from_value::<RecordingCatalogUri>(json!(entry)).unwrap(),
            entry
        );
        let archive = PlaybackArchiveUri::new(&origin, dataset, recording);
        assert_eq!(
            archive.as_str(),
            format!("{expected}/dataset/019ABCDEABCD7ABC8abcabcdefabcdef?segment_id={recording}")
        );
        assert_eq!(archive.dataset_id(), dataset);
        assert_eq!(archive.recording_id(), recording);
        assert_eq!(
            PlaybackArchiveUri::parse(archive.as_str()).unwrap(),
            archive
        );
        assert_eq!(
            serde_json::from_value::<PlaybackArchiveUri>(json!(archive)).unwrap(),
            archive
        );
    }
}

#[test]
fn redap_admission_rejects_ambiguous_or_unqualified_addresses_without_echoing_input() {
    let origin = RecordingRedapOrigin::from_http("https://example.com").unwrap();
    let entry = RecordingCatalogUri::new(&origin, dataset()).to_string();
    let recording = RecordingId::new();
    let archive = PlaybackArchiveUri::new(&origin, dataset(), recording).to_string();
    for value in [
        entry.replace("rerun:", "https:"),
        entry.replace("rerun:", "rerun+https:"),
        entry.replace(":443", ""),
        entry.replace(":443", ":0"),
        entry.replace(":443", ":0443"),
        entry.replace("example.com", "EXAMPLE.com"),
        entry.replace("example.com", "%65xample.com"),
        entry.replace("example.com", "user:secret@example.com"),
        entry.replace("example.com", "[::]"),
        entry.replace("example.com", "0.0.0.0"),
        entry.replace("/entry/", "/dataset/"),
        entry.replace("/entry/", "/anything/../entry/"),
        entry.replace("019ABCDE", "%3019ABCDE"),
        entry.replace("019ABCDE", "019abcde"),
        entry.replace("7ABC8abc", "4ABC8abc"),
        entry.replace("7ABC8abc", "7ABC1abc"),
        entry.replace("019ABCDEABCD7ABC8abcabcdefabcdef", &dataset().to_string()),
        format!("{entry}/"),
        format!("{entry}?token=secret"),
        format!("{entry}#token=secret"),
        format!("\n{entry}"),
    ] {
        let error = RecordingCatalogUri::parse(&value).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid Recording Redap address or HTTP(S) origin"
        );
        assert!(serde_json::from_value::<RecordingCatalogUri>(json!(value)).is_err());
    }
    for value in [
        archive.replace("segment_id", "partition_id"),
        archive.replace("segment_id", "segment%5fid"),
        archive.replace("dataset/", "dataset//"),
        archive.replace("?segment_id", "/assets?segment_id"),
        archive.replace(
            &recording.to_string(),
            &recording.to_string().replace('-', ""),
        ),
        format!("{archive}&segment_id={recording}"),
        format!("{archive}&token=secret"),
        format!("{archive}#timeline=tick"),
        archive.split('?').next().unwrap().to_owned(),
    ] {
        assert!(PlaybackArchiveUri::parse(&value).is_err(), "{value}");
        assert!(serde_json::from_value::<PlaybackArchiveUri>(json!(value)).is_err());
    }
    for http in [
        "ftp://example.com",
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com?token=secret",
        "https://example.com#fragment",
        "https://[::]",
        "http://0.0.0.0",
        "http://localhost:0",
        "\nhttps://example.com",
    ] {
        assert!(RecordingRedapOrigin::from_http(http).is_err());
    }
}

fn grant_builder() -> RecordingCatalogGrantBuilder {
    RecordingCatalogGrantBuilder {
        schema: RecordingCatalogGrantSchema::V1,
        grant_id: RecordingReadGrantId::new(),
        dataset_id: dataset(),
        recording_segment_ids: vec![RecordingId::new()],
        catalog_revision: "catalog-1".to_owned(),
        entry_uri: RecordingCatalogUri::new(
            &RecordingRedapOrigin::from_http("https://example.com").unwrap(),
            dataset(),
        ),
        redap_token: "opaque-token".to_owned(),
        expires_at: "2026-09-29T06:00:00Z".parse().unwrap(),
    }
}

#[test]
fn catalog_grants_admit_dataset_selection_version_and_expiry_before_exposure() {
    let grant = grant_builder().build().unwrap();
    let wire = serde_json::to_value(&grant).unwrap();
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<RecordingCatalogGrant>(wire.clone()).unwrap()
        )
        .unwrap(),
        wire
    );
    assert_eq!(grant.entry_uri.dataset_id(), grant.dataset_id);
    let schema = serde_json::to_value(schemars::schema_for!(RecordingCatalogGrant)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    for (field, value) in [
        ("dataset_id", json!(RecordingDatasetId::new())),
        (
            "entry_uri",
            json!(RecordingCatalogUri::new(
                &RecordingRedapOrigin::from_http("https://example.com").unwrap(),
                RecordingDatasetId::new()
            )),
        ),
        ("recording_segment_ids", json!([])),
        (
            "recording_segment_ids",
            json!(vec![grant.recording_segment_ids[0]; 2]),
        ),
        ("schema", json!("veoveo.ai/recording-catalog-grant/v2")),
        ("catalog_revision", json!(" ")),
        ("catalog_revision", json!("a".repeat(129))),
        ("redap_token", json!("")),
        ("redap_token", json!("secret\nvalue")),
        ("expires_at", json!("not-a-date")),
        ("unknown", json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<RecordingCatalogGrant>(invalid.clone()).is_err(),
            "{field}"
        );
        if let Ok(builder) = serde_json::from_value::<RecordingCatalogGrantBuilder>(invalid) {
            assert!(builder.build().is_err(), "{field}");
        }
    }
    let mut builder = grant_builder();
    builder.recording_segment_ids = (0..500).map(|_| RecordingId::new()).collect();
    builder.recording_segment_ids.sort_unstable();
    assert!(builder.clone().build().is_ok());
    builder.recording_segment_ids.reverse();
    assert!(builder.clone().build().is_err());
    builder.recording_segment_ids.push(RecordingId::new());
    builder.recording_segment_ids.sort_unstable();
    assert!(builder.build().is_err());
}
