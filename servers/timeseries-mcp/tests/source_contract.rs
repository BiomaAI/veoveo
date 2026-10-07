use serde_json::json;
use veoveo_timeseries_mcp::contract::*;

#[test]
fn forecasting_consumes_source_admission_from_the_owner() {
    let request = |source| {
        json!({
            "source":source,"mapping":{"valueColumn":"value"},
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
    let current = serde_json::to_value(schemars::schema_for!(TimeseriesForecastRequest)).unwrap();
    if let Some(output) = std::env::var_os("VEOVEO_CAPTURE_TIMESERIES_SOURCE_SCHEMA") {
        use std::io::Write;
        let output = std::path::PathBuf::from(output);
        assert!(output.is_absolute(), "schema capture path must be absolute");
        let bytes = serde_json::to_vec_pretty(&current).unwrap();
        assert!(
            bytes.len() <= 4 * 1024 * 1024,
            "schema capture exceeds 4 MiB"
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.write_all(b"\n").unwrap();
    }
    assert_eq!(current, expected);
}

#[test]
fn forecast_schema_and_decoder_enforce_the_same_request_profile() {
    let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastRequest)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let base = json!({"source":{"kind":"inline_csv","csv":"value\n1\n"},"mapping":{"valueColumn":"value"},"horizon":4});
    let check = |value: serde_json::Value, valid| {
        assert_eq!(validator.is_valid(&value), valid, "schema: {value}");
        assert_eq!(
            serde_json::from_value::<TimeseriesForecastRequest>(value.clone()).is_ok(),
            valid,
            "decoder: {value}"
        );
    };
    check(base.clone(), true);
    for (horizon, valid) in [(0, false), (1, true), (100_000, true), (100_001, false)] {
        let mut value = base.clone();
        value["horizon"] = json!(horizon);
        check(value, valid);
    }
    for column in ["valueColumn", "timeColumn", "seriesColumn"] {
        for (name, valid) in [
            ("", false),
            (" \n\t", false),
            ("\u{2003}", false),
            ("a\0b", false),
            (" quoted \"column\" ", true),
        ] {
            let mut value = base.clone();
            value["mapping"][column] = json!(name);
            check(value, valid);
        }
    }
    for (filter, valid) in [
        (json!({"predicates":[]}), false),
        (
            json!({"predicates":[{"op":"in","column":"value","values":[]}]}),
            false,
        ),
        (
            json!({"predicates":[{"op":"in","column":"value","values":[null]}]}),
            false,
        ),
        (
            json!({"predicates":[{"op":"in","column":"value","values":[1,2.5,"3",true]}]}),
            true,
        ),
        (
            json!({"predicates":[{"op":"eq","column":"","value":1}]}),
            false,
        ),
        (
            json!({"predicates":[{"op":"eq","column":"value","value":1,"extra":true}]}),
            false,
        ),
        (
            json!({"predicates":[{"op":"is_not_null","column":"value"}],"extra":true}),
            false,
        ),
        (
            json!({"combination":"any","predicates":[{"op":"ne","column":"value","value":1},{"op":"is_not_null","column":"value"}]}),
            true,
        ),
    ] {
        let mut value = base.clone();
        value["trainingFilter"] = filter;
        check(value, valid);
    }
    let mut artifact = base.clone();
    artifact["source"] = json!({"kind":"artifact","uri":"artifact://01983da0-0000-7000-8000-000000000001","format":"csv"});
    check(artifact, false);
    let mut unknown = base;
    unknown["unhandled"] = json!(true);
    check(unknown, false);
}

#[test]
fn native_construction_cannot_create_invalid_horizons_or_filters() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            TimeseriesFilterNumber::new(invalid),
            Err(TimeseriesRequestError::NonFiniteFilter)
        );
    }
    assert_eq!(
        TimeseriesForecastHorizon::new(0),
        Err(TimeseriesRequestError::Horizon)
    );
    assert_eq!(
        TimeseriesForecastHorizon::new(100_001),
        Err(TimeseriesRequestError::Horizon)
    );
    assert_eq!(
        TimeseriesFilterValues::try_from(vec![]),
        Err(TimeseriesRequestError::EmptyFilterValues)
    );
    let number = TimeseriesFilterNumber::new(1.25).unwrap();
    assert_eq!(
        serde_json::to_value(TimeseriesFilterValue::F64(number)).unwrap(),
        json!(1.25)
    );
}

#[test]
fn forecast_request_refuses_retired_mapping_filter_and_nested_reader_names() {
    let current = json!({"source":{"kind":"inline_csv","csv":"value\n1\n", "options":{"timestampFormat":"%Y"}},
        "mapping":{"valueColumn":"value", "timeColumn":"timestamp"}, "horizon":4,
        "trainingFilter":{"predicates":[{"op":"is_not_null","column":"value"}]}});
    let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastRequest)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&current));
    serde_json::from_value::<TimeseriesForecastRequest>(current.clone()).unwrap();
    for (parent, canonical, retired) in [
        ("", "trainingFilter", "training_filter"),
        ("/mapping", "valueColumn", "value_column"),
        ("/mapping", "timeColumn", "time_column"),
        ("/source/options", "timestampFormat", "timestamp_format"),
    ] {
        for mixed in [false, true] {
            let mut bad = current.clone();
            let object = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
            let value = object[canonical].clone();
            object.insert(retired.into(), value);
            if !mixed {
                object.remove(canonical);
            }
            assert!(!validator.is_valid(&bad));
            assert!(serde_json::from_value::<TimeseriesForecastRequest>(bad).is_err());
        }
    }
}
