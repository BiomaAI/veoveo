use schemars::JsonSchema;
use serde::Serialize;

mod first_server {
    veoveo_types::scope_enum! { pub enum Scope { Read => "first:read" } }
}

mod second_server {
    veoveo_types::scope_enum! { pub enum Scope { Read => "second:read" } }
}

mod empty_server {
    veoveo_types::scope_enum! { pub enum Scope {} }
}

#[test]
fn empty_vocabulary_rejects_every_name_and_has_an_uninhabited_schema() {
    use empty_server::Scope;
    use veoveo_types::ScopeName;
    assert!(Scope::ALL.is_empty());
    for name in ["", "other:read", "two scopes", "openid"] {
        assert!(name.parse::<Scope>().is_err());
        assert!(serde_json::from_value::<Scope>(serde_json::json!(name)).is_err());
    }
    assert!(Scope::try_from(&ScopeName::new("other:read").unwrap()).is_err());
    assert_eq!(
        serde_json::to_value(Scope::json_schema(&mut schemars::SchemaGenerator::default()))
            .unwrap(),
        serde_json::json!(false)
    );
    assert_ne!(Scope::schema_id(), first_server::Scope::schema_id());
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
