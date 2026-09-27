use schemars::JsonSchema;
use serde::Serialize;

mod first_server {
    veoveo_types::scope_enum! { pub enum Scope { Read => "first:read" } }
}

mod second_server {
    veoveo_types::scope_enum! { pub enum Scope { Read => "second:read" } }
}

#[derive(JsonSchema, Serialize)]
struct Combined {
    first: first_server::Scope,
    second: second_server::Scope,
}

#[test]
fn independently_owned_same_named_enums_keep_distinct_schemas() {
    assert_ne!(
        first_server::Scope::schema_id(),
        second_server::Scope::schema_id()
    );
    let schema = serde_json::to_value(schemars::schema_for!(Combined)).unwrap();
    let first = schema["properties"]["first"]["$ref"].as_str().unwrap();
    let second = schema["properties"]["second"]["$ref"].as_str().unwrap();
    assert_ne!(first, second);
    assert_eq!(
        schema.pointer(first.strip_prefix('#').unwrap()).unwrap()["enum"],
        serde_json::json!(["first:read"])
    );
    assert_eq!(
        schema.pointer(second.strip_prefix('#').unwrap()).unwrap()["enum"],
        serde_json::json!(["second:read"])
    );
    let value = Combined {
        first: first_server::Scope::Read,
        second: second_server::Scope::Read,
    };
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({"first":"first:read","second":"second:read"})
    );
}
