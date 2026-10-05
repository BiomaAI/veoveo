use schemars::JsonSchema;
use veoveo_types::{TaskTypeDefinition, Vocabulary};

mod owner_result_alias {
    type Result<T> = std::result::Result<T, &'static str>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
    #[vocabulary(scope)]
    enum Permission {
        #[vocabulary(rename = "owner:read")]
        Read,
    }

    #[test]
    fn generated_results_do_not_resolve_to_the_owner_alias() {
        let result: Result<Permission> = "owner:read".parse().map_err(|_| "invalid permission");
        let permission = result.unwrap();
        assert_eq!(
            serde_json::to_string(&permission).unwrap(),
            "\"owner:read\""
        );
        assert_eq!(
            serde_json::from_str::<Permission>("\"owner:read\"").unwrap(),
            permission
        );
        assert_eq!(
            Permission::try_from(&veoveo_types::ScopeName::parse("owner:read").unwrap()).unwrap(),
            permission
        );
    }
}

/// States described by an independent owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Vocabulary)]
#[schemars(
    rename = "OwnerState",
    description = "Owner-selected schema description."
)]
enum State {
    /// Waiting for owner admission.
    Waiting,
    #[vocabulary(rename = "run-ready")]
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Vocabulary)]
#[vocabulary(task_type)]
enum Operation {
    #[vocabulary(rename = "domain.first")]
    First,
    #[vocabulary(rename = "domain_second")]
    Second,
}

mod baseline {
    /// States described by an independent owner.
    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    #[schemars(
        rename = "OwnerState",
        description = "Owner-selected schema description."
    )]
    #[serde(rename_all = "snake_case")]
    pub enum State {
        /// Waiting for owner admission.
        Waiting,
        #[serde(rename = "run-ready")]
        Ready,
    }
}

mod first_owner {
    /// First owner's states.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
    pub enum Status {
        /// First owner is ready.
        Ready,
    }
}

mod second_owner {
    /// Second owner's states.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
    pub enum Status {
        /// Second owner is waiting.
        Waiting,
    }
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct Combined {
    first: first_owner::Status,
    second: second_owner::Status,
}

#[test]
fn same_named_owner_schemas_keep_ids_documentation_and_references() {
    assert_eq!(
        first_owner::Status::schema_id(),
        concat!(module_path!(), "::first_owner::Status")
    );
    assert_eq!(
        second_owner::Status::schema_id(),
        concat!(module_path!(), "::second_owner::Status")
    );
    let schema = serde_json::to_value(schemars::schema_for!(Combined)).unwrap();
    let first = schema["properties"]["first"]["$ref"].as_str().unwrap();
    let second = schema["properties"]["second"]["$ref"].as_str().unwrap();
    assert_ne!(first, second);
    let first = schema.pointer(first.strip_prefix('#').unwrap()).unwrap();
    let second = schema.pointer(second.strip_prefix('#').unwrap()).unwrap();
    assert_eq!(first["description"], "First owner's states.");
    assert_eq!(first["oneOf"][0]["description"], "First owner is ready.");
    assert_eq!(first["oneOf"][0]["const"], "ready");
    assert_eq!(second["description"], "Second owner's states.");
    assert_eq!(
        second["oneOf"][0]["description"],
        "Second owner is waiting."
    );
    assert_eq!(second["oneOf"][0]["const"], "waiting");
}

#[test]
fn every_surface_uses_the_owners_spelling() {
    assert_eq!(State::ALL, &[State::Waiting, State::Ready]);
    for state in State::ALL {
        let spelling = state.as_str();
        assert_eq!(State::from_wire(spelling), Some(*state));
        assert_eq!(state.to_string(), spelling);
        assert_eq!(spelling.parse::<State>().unwrap(), *state);
        assert_eq!(serde_json::to_value(state).unwrap(), spelling);
        assert_eq!(
            serde_json::from_value::<State>(spelling.into()).unwrap(),
            *state
        );
    }
    for unknown in ["", "Ready", "unregistered", " ready "] {
        assert!(unknown.parse::<State>().is_err());
        assert!(serde_json::from_value::<State>(unknown.into()).is_err());
    }
    assert_eq!(
        schemars::schema_for!(State),
        schemars::schema_for!(baseline::State)
    );
    assert_eq!(State::schema_id(), concat!(module_path!(), "::OwnerState"));
}

#[test]
fn task_hook_preserves_complete_distinct_admission() {
    assert_eq!(Operation::ALL, <Operation as TaskTypeDefinition>::ALL);
    for operation in Operation::ALL {
        assert_eq!(operation.name().as_str(), operation.as_str());
        assert_eq!(Operation::from_name(&operation.name()), Some(*operation));
        assert_eq!(
            Operation::from_wire_name(operation.as_str()),
            Some(*operation)
        );
    }
    assert_eq!(Operation::from_wire_name("unregistered"), None);
    assert_eq!(Operation::from_wire_name("Domain.First"), None);
    assert_eq!(Operation::from_wire_name("not a task name"), None);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Independent {
    Read,
}

impl Vocabulary for Independent {
    const ALL: &'static [Self] = &[Self::Read];
    const NAME: &'static str = "Independent";
    fn as_str(self) -> &'static str {
        "independent:read"
    }
}

#[test]
fn independently_implemented_trait_needs_no_macro_or_core_registration() {
    assert_eq!(
        Independent::parse("independent:read").unwrap(),
        Independent::Read
    );
    assert!(Independent::parse("other:read").is_err());
}

mod raw_names {
    #![allow(non_camel_case_types)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
    enum r#type {
        r#type,
        HTTPReady,
    }
    mod before {
        #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum r#type {
            r#type,
            HTTPReady,
        }
    }
    #[test]
    fn raw_names_and_acronyms_match_serde_and_schemars() {
        assert_eq!(r#type::r#type.as_str(), "type");
        assert_eq!(r#type::HTTPReady.as_str(), "h_t_t_p_ready");
        assert_eq!(
            serde_json::to_value(r#type::r#type).unwrap(),
            serde_json::to_value(before::r#type::r#type).unwrap()
        );
        assert_eq!(
            serde_json::to_value(r#type::HTTPReady).unwrap(),
            serde_json::to_value(before::r#type::HTTPReady).unwrap()
        );
        assert_eq!(
            schemars::schema_for!(r#type),
            schemars::schema_for!(before::r#type)
        );
        assert_eq!(<r#type as veoveo_types::Vocabulary>::NAME, "type");
    }
}
