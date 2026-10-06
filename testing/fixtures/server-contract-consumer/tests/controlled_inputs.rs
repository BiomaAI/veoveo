//! Full branch admission through public contract-only types and their wire decoders.
#[path = "../../tool_inputs.rs"]
mod fixture;
#[path = "controlled_inputs/owners.rs"]
mod owners;
use fixture::ToolInputCase;
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};

fn qualify<T: JsonSchema + DeserializeOwned + Serialize>(case: &ToolInputCase) {
    let _: T = case.decode();
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    jsonschema::meta::validate(&schema).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(
        validator.is_valid(&case.arguments),
        "{} baseline schema: {:?}",
        case.branch,
        validator.iter_errors(&case.arguments).collect::<Vec<_>>()
    );
    for (pointer, value) in case.omitted_values() {
        assert!(
            validator.is_valid(&value),
            "{} schema rejected optional omission {pointer}",
            case.branch
        );
        let _: T = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    }
    for value in case.open_values() {
        assert!(
            validator.is_valid(&value),
            "{} schema rejected open map",
            case.branch
        );
    }
    for (where_, value) in case
        .unknown_fields()
        .into_iter()
        .chain(case.invalid_values())
    {
        assert!(
            !validator.is_valid(&value),
            "{} schema admitted {where_}",
            case.branch
        );
        match serde_json::from_slice::<T>(&serde_json::to_vec(&value).unwrap()) {
            Ok(_) => panic!("{} wire decoder admitted {where_}", case.branch),
            Err(error) => case.assert_error(&where_, &error.to_string()),
        }
    }
}

fn assert_branches(cases: &[ToolInputCase], expected: &[&str]) {
    let actual: std::collections::BTreeSet<_> =
        cases.iter().map(|case| case.branch.as_str()).collect();
    assert_eq!(actual, expected.iter().copied().collect());
}
