use std::{collections::BTreeMap, io::Cursor};

use crate::{ExecutionRequest, LaunchError, MAX_FRAME_BYTES, read_frame};

fn json_frame(json: &str) -> Vec<u8> {
    let mut frame = (json.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(json.as_bytes());
    frame
}

fn request(arguments: Vec<String>, directory: &str) -> Result<ExecutionRequest, LaunchError> {
    ExecutionRequest::new(arguments, directory.to_owned(), BTreeMap::new(), vec![])
}

#[test]
fn framing_preserves_argv_multiline_environment_binary_stdin_and_trailing_transport() {
    let arguments = vec![
        "printf".into(),
        String::new(),
        "a 'quoted' value\nwith á".into(),
    ];
    let environment = BTreeMap::from([("MULTILINE".to_owned(), "first\nlast".to_owned())]);
    let input = vec![0, 255, 13, 10, 42];
    let original = ExecutionRequest::new(
        arguments.clone(),
        "project".into(),
        environment.clone(),
        input.clone(),
    )
    .unwrap();
    let encoded = original.encode().unwrap();
    assert_eq!(
        u32::from_be_bytes(encoded[..4].try_into().unwrap()) as usize,
        encoded.len() - 4
    );
    let mut with_tail = encoded.to_vec();
    with_tail.extend_from_slice(b"transport stays open");
    let mut cursor = Cursor::new(with_tail);
    let decoded = read_frame(&mut cursor).unwrap();
    assert_eq!(cursor.position() as usize, encoded.len());
    assert_eq!(decoded.0.arguments, arguments);
    assert_eq!(decoded.0.environment, environment);
    assert_eq!(decoded.0.stdin, input);
    assert_eq!(decoded.encode().unwrap().as_slice(), encoded.as_slice());
}

#[test]
fn malformed_versions_duplicate_fields_and_duplicate_environment_keys_are_rejected() {
    for json in [
        r#"{"version":2,"arguments":["true"],"directory":".","environment":{},"stdin":""}"#,
        r#"{"version":1,"arguments":["true"],"directory":".","environment":{},"stdin":"","owner":"forged"}"#,
        r#"{"version":1,"version":1,"arguments":["true"],"directory":".","environment":{},"stdin":""}"#,
        r#"{"version":1,"arguments":["true"],"directory":".","environment":{"A":"one","A":"two"},"stdin":""}"#,
        r#"{"version":1,"arguments":["true"],"directory":".","environment":{"A":"one","\u0041":"two"},"stdin":""}"#,
        r#"{"version":1,"arguments":["true"],"directory":".","environment":{},"stdin":"!"}"#,
        r#"{"version":1,"arguments":["true"],"directory":".","environment":{},"stdin":""} {}"#,
    ] {
        assert!(matches!(
            read_frame(json_frame(json).as_slice()),
            Err(LaunchError::InvalidRequest)
        ));
    }
}

#[test]
fn frame_length_is_checked_before_allocation_and_truncated_payloads_fail() {
    for length in [0, MAX_FRAME_BYTES as u32 + 1, u32::MAX] {
        assert!(read_frame(length.to_be_bytes().as_slice()).is_err());
    }
    let encoded = request(vec!["true".into()], ".").unwrap().encode().unwrap();
    for length in [0, 3, encoded.len() - 1] {
        assert!(read_frame(&encoded[..length]).is_err());
    }
}

#[test]
fn aggregate_argument_bounds_accept_empty_values_without_unbounded_allocation() {
    let mut arguments = vec!["true".into()];
    arguments.resize(1024, String::new());
    let encoded = request(arguments.clone(), ".").unwrap().encode().unwrap();
    assert_eq!(
        read_frame(encoded.as_slice()).unwrap().0.arguments.len(),
        1024
    );
    arguments.push(String::new());
    assert!(request(arguments, ".").is_err());
    let mut excessive = serde_json::json!({"version":1,"arguments":vec!["";1025],"directory":".","environment":{},"stdin":""});
    excessive["arguments"][0] = "true".into();
    assert!(read_frame(json_frame(&excessive.to_string()).as_slice()).is_err());
    assert!(request(vec!["true".into(), "x".repeat(32764)], ".").is_ok());
    assert!(request(vec!["true".into(), "x".repeat(32765)], ".").is_err());
    assert!(request(vec![String::new()], ".").is_err());
    assert!(request(vec!["true".into(), "a\0b".into()], ".").is_err());
}

#[test]
fn directories_environment_and_finite_input_have_independent_bounds() {
    for directory in ["", "/etc", "..", "project/../outside", "project\nname"] {
        assert!(request(vec!["true".into()], directory).is_err());
    }
    for name in ["", "1KEY", "KEY=VALUE", "KEY\n", "é"] {
        assert!(
            ExecutionRequest::new(
                vec!["true".into()],
                ".".into(),
                BTreeMap::from([(name.into(), "value".into())]),
                vec![]
            )
            .is_err()
        );
    }
    assert!(
        ExecutionRequest::new(
            vec!["true".into()],
            ".".into(),
            BTreeMap::from([("KEY".into(), "value\0".into())]),
            vec![]
        )
        .is_err()
    );
    let input = vec![255; 1024 * 1024];
    let original = ExecutionRequest::new(
        vec!["true".into()],
        ".".into(),
        BTreeMap::new(),
        input.clone(),
    )
    .unwrap();
    assert_eq!(
        read_frame(original.encode().unwrap().as_slice())
            .unwrap()
            .0
            .stdin,
        input
    );
    assert!(
        ExecutionRequest::new(
            vec!["true".into()],
            ".".into(),
            BTreeMap::new(),
            vec![0; 1024 * 1024 + 1]
        )
        .is_err()
    );
}
