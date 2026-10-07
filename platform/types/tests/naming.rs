use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use veoveo_types::*;

fn admitted(profile: ScalarNaming, text: &str) -> bool {
    matches!(
        profile.check_spelling(text),
        Ok(veoveo_types::naming::SpellingCheck::BuiltinValidated)
    )
}
fn declaration() -> NamingDeclaration {
    NamingDeclaration {
        authority: NamingAuthority::Owner {
            module: NamingLabel::new("independent_owner::receipt").unwrap(),
        },
        profile: NamingLabel::new("receipt").unwrap(),
        version: NamingLabel::new("v1").unwrap(),
        applicability: NamingLabel::new("only this retained receipt object").unwrap(),
    }
}
fn builtin(grammar: ScalarGrammar) -> NamingProfile {
    NamingProfile::new(NamingRole::Scalar {
        profile: ScalarNaming::builtin(grammar),
    })
    .unwrap()
}
#[test]
fn closed_profile_round_trips_and_rejects_unknown_metadata() {
    for role in [
        NamingRole::Scalar {
            profile: ScalarNaming::builtin(ScalarGrammar::ScopeToken),
        },
        NamingRole::Jwt {
            declaration: declaration(),
        },
        NamingRole::Frozen {
            declaration: declaration(),
        },
        NamingRole::External {
            declaration: declaration(),
        },
    ] {
        let admitted = NamingProfile::new(role).unwrap();
        assert_eq!(
            serde_json::from_value::<NamingProfile>(serde_json::to_value(&admitted).unwrap())
                .unwrap(),
            admitted
        );
    }
    let valid = serde_json::to_value(builtin(ScalarGrammar::ScopeToken)).unwrap();
    for bad in [
        json!(null),
        json!({"revision":2,"role":{"kind":"scalar","profile":{"kind":"builtin","grammar":"scope_token"}}}),
        json!({"revision":1,"role":{"kind":"ignore"}}),
        json!({"revision":1,"role":{"kind":"scalar","profile":{"kind":"builtin","grammar":"arbitrary-regex"}}}),
    ] {
        assert!(serde_json::from_value::<NamingProfile>(bad).is_err());
    }
    for pointer in ["", "/role", "/role/profile"] {
        let mut extra = valid.clone();
        extra
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        assert!(serde_json::from_value::<NamingProfile>(extra).is_err());
    }
    for bad in ["", "  ", "version\n2"] {
        assert!(NamingLabel::new(bad).is_err());
    }
    assert!(serde_json::from_value::<NamingLabel>(json!("\t")).is_err());
}
#[test]
fn builtins_preserve_existing_scope_task_resource_and_format_grammars() {
    assert!(admitted(
        ScalarNaming::builtin(ScalarGrammar::ScopeName),
        "機密"
    ));
    assert!(!admitted(
        ScalarNaming::builtin(ScalarGrammar::ScopeToken),
        "機密"
    ));
    assert!(admitted(
        ScalarNaming::builtin(ScalarGrammar::TaskTypeName),
        "owner.file_transfer"
    ));
    assert!(admitted(
        ScalarNaming::builtin(ScalarGrammar::FormatTag),
        "veoveo.ai/owner/nested-format/v2"
    ));
    for (grammar, values) in [
        (
            ScalarGrammar::ScopeToken,
            vec!["two scopes", "read\\write", "read\"write"],
        ),
        (ScalarGrammar::ScopeName, vec!["two scopes", "scope\n"]),
        (
            ScalarGrammar::TaskTypeName,
            vec!["Owner.operation", "two tasks"],
        ),
        (ScalarGrammar::ResourceScheme, vec!["Upper", "0bad"]),
        (
            ScalarGrammar::ResourceUri,
            vec!["not a URI", "owner://items/{id}"],
        ),
        (
            ScalarGrammar::ResourceTemplate,
            vec!["owner://{broken", "HTTP://example"],
        ),
        (
            ScalarGrammar::FormatTag,
            vec![
                concat!("io", ".veoveo/example/v1"),
                "veoveo.ai/example/v0",
                "veoveo.ai/example/v01",
            ],
        ),
    ] {
        for value in values {
            assert!(
                !admitted(ScalarNaming::builtin(grammar), value),
                "{grammar:?}: {value}"
            );
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Vocabulary)]
#[vocabulary(scope)]
enum OwnerScope {
    #[vocabulary(rename = "independent-owner:read")]
    Read,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Vocabulary)]
#[vocabulary(task_type)]
enum OwnerTask {
    #[vocabulary(rename = "independent_owner.run-item")]
    Run,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Vocabulary)]
enum Ordinary {
    ReadyToRun,
}
#[test]
fn derive_hooks_and_dynamic_types_emit_only_their_actual_profile() {
    let mut generator = SchemaGenerator::default();
    assert_eq!(
        naming_profile(
            &OwnerScope::json_schema(&mut generator),
            veoveo_types::NamingSchemaContext::new(&json_schema!({}))
        )
        .unwrap()
        .unwrap(),
        builtin(ScalarGrammar::ScopeToken)
    );
    assert_eq!(
        naming_profile(
            &OwnerTask::json_schema(&mut generator),
            veoveo_types::NamingSchemaContext::new(&json_schema!({}))
        )
        .unwrap()
        .unwrap(),
        builtin(ScalarGrammar::TaskTypeName)
    );
    assert!(
        Ordinary::json_schema(&mut generator)
            .get(NAMING_PROFILE_KEY)
            .is_none()
    );
    assert_eq!(
        serde_json::to_value(OwnerScope::Read).unwrap(),
        "independent-owner:read"
    );
    assert_eq!(
        serde_json::to_value(OwnerTask::Run).unwrap(),
        "independent_owner.run-item"
    );
    fn actual<T: JsonSchema>(grammar: ScalarGrammar) {
        let root = schemars::schema_for!(T);
        assert_eq!(
            naming_profile(&root, veoveo_types::NamingSchemaContext::new(&root))
                .unwrap()
                .unwrap(),
            builtin(grammar)
        );
    }
    actual::<ScopeName>(ScalarGrammar::ScopeName);
    actual::<TaskTypeName>(ScalarGrammar::TaskTypeName);
    actual::<ResourceScheme>(ScalarGrammar::ResourceScheme);
    actual::<ResourceUri>(ScalarGrammar::ResourceUri);
    actual::<ResourceTemplateUri>(ScalarGrammar::ResourceTemplate);
    actual::<ResourceUriTemplate>(ScalarGrammar::ResourceSelectorTemplate);
    actual::<ResourceUriPrefix>(ScalarGrammar::ResourcePrefix);
    actual::<ServerSlug>(ScalarGrammar::ServerSlug);
    actual::<ExtensionName>(ScalarGrammar::ExtensionName);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Vocabulary)]
enum Metric {
    TravelTime,
    RouteVariance,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Amount {
    #[schemars(range(min = 1))]
    observed_count: u64,
}
impl Check for Amount {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        if self.observed_count > 0 {
            Ok(())
        } else {
            Err("count must be positive")
        }
    }
}
#[test]
fn finite_dictionary_uses_the_live_key_schema_and_preserves_checked_values() {
    for settings in [
        schemars::generate::SchemaSettings::draft2020_12(),
        schemars::generate::SchemaSettings::draft07(),
    ] {
        let mut generator = settings.into_generator();
        let map = <BTreeMap<Metric, Checked<Amount>>>::json_schema(&mut generator);
        let decorated = dictionary_schema::<Metric>(&mut generator, map.clone()).unwrap();
        let root = generator.root_schema_for::<BTreeMap<Metric, Checked<Amount>>>();
        let profile = naming_profile(&decorated, veoveo_types::NamingSchemaContext::new(&root))
            .unwrap()
            .unwrap();
        let NamingRole::Dictionary { key_schema } = profile.role() else {
            panic!("dictionary role")
        };
        assert_eq!(key_schema, &generator.subschema_for::<Metric>());
        let mut constraints = decorated.as_value().clone();
        constraints
            .as_object_mut()
            .unwrap()
            .remove(NAMING_PROFILE_KEY);
        assert_eq!(constraints, map.as_value().clone());
        for key in ["extra_key", "travel_time"] {
            let mut corrupt = decorated.clone();
            let props = corrupt
                .get_mut("properties")
                .unwrap()
                .as_object_mut()
                .unwrap();
            if key == "travel_time" {
                props.remove(key);
            } else {
                props.insert(key.into(), json!({"type":"string"}));
            }
            assert!(
                naming_profile(&corrupt, veoveo_types::NamingSchemaContext::new(&root)).is_err()
            );
        }
        let bytes = json!({"travel_time":{"observedCount":1}});
        let admitted: BTreeMap<Metric, Checked<Amount>> =
            serde_json::from_value(bytes.clone()).unwrap();
        assert_eq!(serde_json::to_value(admitted).unwrap(), bytes);
        assert!(
            serde_json::from_value::<BTreeMap<Metric, Checked<Amount>>>(
                json!({"travel_time":{"observedCount":0}})
            )
            .is_err()
        );
    }
}
#[test]
fn open_dictionary_and_narrow_subtrees_do_not_change_siblings_or_values() {
    let mut generator = SchemaGenerator::default();
    let map = <BTreeMap<String, Amount>>::json_schema(&mut generator);
    let annotated = dictionary_schema::<String>(&mut generator, map.clone()).unwrap();
    assert_eq!(
        annotated.get("additionalProperties"),
        map.get("additionalProperties")
    );
    for role in [
        NamingRole::Jwt {
            declaration: declaration(),
        },
        NamingRole::Frozen {
            declaration: declaration(),
        },
        NamingRole::External {
            declaration: declaration(),
        },
    ] {
        let child = json_schema!({"type":"object","properties":{"preserved_field":{"type":"string"}},"additionalProperties":false});
        let child = with_naming_profile(
            child.clone(),
            NamingProfile::new(role).unwrap(),
            veoveo_types::NamingSchemaContext::new(&child),
        )
        .unwrap();
        let outer = json_schema!({"type":"object","properties":{"child":child,"wrong_outer":{"type":"string","enum":["provider-wait"]}},"additionalProperties":false});
        assert!(
            naming_profile(&outer, veoveo_types::NamingSchemaContext::new(&outer))
                .unwrap()
                .is_none()
        );
        assert!(
            outer.get("properties").unwrap()["wrong_outer"]
                .get(NAMING_PROFILE_KEY)
                .is_none()
        );
    }
}
#[test]
fn wrong_node_kinds_conflicts_and_external_or_unresolved_key_links_fail() {
    let scalar = json_schema!({"type":"string","enum":["owner:read"]});
    let scalar = with_naming_profile(
        scalar.clone(),
        builtin(ScalarGrammar::ScopeToken),
        veoveo_types::NamingSchemaContext::new(&scalar),
    )
    .unwrap();
    assert!(
        with_naming_profile(
            scalar.clone(),
            builtin(ScalarGrammar::TaskTypeName),
            veoveo_types::NamingSchemaContext::new(&scalar)
        )
        .is_err()
    );
    assert!(scalar_schema(scalar, ScalarNaming::builtin(ScalarGrammar::TaskTypeName)).is_err());
    let object = json_schema!({"type":"object","properties":{"optional_field":{"type":"string"}},"additionalProperties":false});
    assert!(
        with_naming_profile(
            object.clone(),
            builtin(ScalarGrammar::ScopeToken),
            veoveo_types::NamingSchemaContext::new(&object)
        )
        .is_err()
    );
    let str_schema = json_schema!({"type":"string"});
    assert!(
        with_naming_profile(
            str_schema.clone(),
            NamingProfile::new(NamingRole::External {
                declaration: declaration()
            })
            .unwrap(),
            veoveo_types::NamingSchemaContext::new(&str_schema)
        )
        .is_err()
    );
    for key in [
        json_schema!({"$ref":"https://external.example/schema"}),
        json_schema!({"$ref":"#/$defs/Missing"}),
        json_schema!({"$ref":"#/$defs/Cycle"}),
    ] {
        let root = json_schema!({"$defs":{"Cycle":{"$ref":"#/$defs/Cycle"}}});
        assert!(
            with_naming_profile(
                object.clone(),
                NamingProfile::new(NamingRole::Dictionary { key_schema: key }).unwrap(),
                veoveo_types::NamingSchemaContext::new(&root)
            )
            .is_err()
        );
    }
}
#[test]
fn callback_constraints_and_explicit_owner_selection_remain_local() {
    let mut generator = SchemaGenerator::default();
    let schema = json_schema!({"type":"string","description":"owner words","pattern":"^receipt:.+$","minLength":9});
    let selected = ScalarNaming::OwnerIdentity {
        declaration: declaration(),
    };
    let classified = scalar_schema(schema.clone(), selected.clone()).unwrap();
    assert_eq!(
        veoveo_types::naming::static_identity_schema(
            classified.clone(),
            Some(selected),
            "independent_owner",
            "Receipt",
            &mut generator
        ),
        classified
    );
    let mut plain = classified.as_value().clone();
    plain.as_object_mut().unwrap().remove(NAMING_PROFILE_KEY);
    assert_eq!(plain, schema.as_value().clone());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            veoveo_types::naming::static_identity_schema(
                classified,
                Some(ScalarNaming::builtin(ScalarGrammar::ScopeToken)),
                "owner",
                "Receipt",
                &mut generator,
            )
        }))
        .is_err()
    );
}

#[test]
fn dictionary_wire_uses_camel_case_metadata_and_refuses_the_old_key() {
    let profile = NamingProfile::new(NamingRole::Dictionary {
        key_schema: json_schema!({"type":"string"}),
    })
    .unwrap();
    let mut wire = serde_json::to_value(profile).unwrap();
    assert!(wire["role"].get("keySchema").is_some());
    let object = wire["role"].as_object_mut().unwrap();
    let key = object.remove("keySchema").unwrap();
    object.insert("key_schema".into(), key);
    assert!(serde_json::from_value::<NamingProfile>(wire).is_err());
    assert!(ScalarNaming::owner("", "receipt").is_err());
    assert!(ScalarNaming::owner_resource("independent_owner", "\n").is_err());
    assert_eq!(
        ScalarNaming::owner("independent_owner", "Receipt")
            .unwrap()
            .check_spelling("unseen bytes")
            .unwrap(),
        veoveo_types::naming::SpellingCheck::DeclaredSchemaRequired
    );
}

// This root is a real scalar definition, rather than a reference to an object DTO.
struct RefScalar;
impl JsonSchema for RefScalar {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "IndependentScalar".into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::IndependentScalar").into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string","description":"actual callback constraints","minLength":1,"maxLength":32,"pattern":"^[a-z]+$"})
    }
}
fn scalar_ref(generator: &mut SchemaGenerator) -> Schema {
    generator.subschema_for::<RefScalar>()
}
fn scalar_union(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf":[generator.subschema_for::<RefScalar>(),{"type":"null"}],"description":"callback union","maxLength":32})
}
fn scalar_array(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type":["string","null"],"description":"callback type array","minLength":1,"maxLength":32})
}
fn admit_id(text: &str) -> Result<(), &'static str> {
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase()) {
        Ok(())
    } else {
        Err("invalid independent identity")
    }
}
#[veoveo_types::id(custom(error = &'static str, validate = admit_id, string, schema = scalar_ref, schema_inline))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
struct LegacyRefId(String);
struct ProfileIds;
impl IdProfile for ProfileIds {
    type Error = &'static str;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|s, _| admit_id(s)).owner_schema(|g, _| scalar_ref(g), true);
}
#[veoveo_types::id(text(ProfileIds))]
struct ProfileRefId(String);
struct AddressProfiles;
impl ResourceProfile for AddressProfiles {
    type Error = ResourceUriError;
    const PROFILE: ResourceProfileSpec<Self::Error> = ResourceProfileSpec {
        route_error: |_, _| ResourceUriError::DisallowedComponent,
    };
    const SCHEMA: Option<ResourceSchema> = Some(ResourceSchema {
        schema: |_, g| scalar_ref(g),
        inline: true,
    });
}
#[veoveo_types::resource_address(custom(template="independent://legacy/{id}", error=ResourceUriError, route_error=|_| ResourceUriError::DisallowedComponent, schema=scalar_ref, schema_inline))]
struct LegacyRefAddress {
    #[resource(variable="id", error=|_| ResourceUriError::DisallowedComponent)]
    id: PrincipalId,
}
#[veoveo_types::resource_address(routes(AddressProfiles), schema=owner, wire)]
enum ProfileRefAddress {
    #[resource(template = "independent://profile")]
    Root,
}
#[veoveo_types::id(custom(error = &'static str, validate = admit_id, string, schema = scalar_union))]
struct UnionId(String);
#[veoveo_types::id(custom(error = &'static str, validate = admit_id, string, schema = scalar_array))]
struct ArrayId(String);
fn raw_graph<T: JsonSchema>(callback: fn(&mut SchemaGenerator) -> Schema) -> Value {
    let mut generator = SchemaGenerator::default();
    let mut schema = callback(&mut generator);
    let root = generator.root_schema_for::<String>();
    let object = schema.ensure_object();
    object.insert("title".into(), T::schema_name().as_ref().into());
    if let Some(v) = root.get("$schema") {
        object.insert("$schema".into(), v.clone());
    }
    if let Some(v) = root.get("$defs") {
        object.insert("$defs".into(), v.clone());
    }
    schema.as_value().clone()
}
#[path = "support/naming.rs"]
mod naming_baseline;
#[test]
fn actual_legacy_and_profile_scalar_ref_callbacks_preserve_full_constraint_graphs() {
    fn actual<T: JsonSchema>(callback: fn(&mut SchemaGenerator) -> Schema) {
        let root = schemars::schema_for!(T);
        assert!(
            root.get("$ref").is_some() || root.get("anyOf").is_some() || root.get("type").is_some()
        );
        assert!(
            naming_profile(&root, veoveo_types::NamingSchemaContext::new(&root))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            naming_baseline::constraints(root.as_value().clone()),
            raw_graph::<T>(callback)
        );
    }
    actual::<LegacyRefId>(scalar_ref);
    actual::<ProfileRefId>(scalar_ref);
    actual::<LegacyRefAddress>(scalar_ref);
    actual::<ProfileRefAddress>(scalar_ref);
    actual::<UnionId>(scalar_union);
    actual::<ArrayId>(scalar_array);
    assert!(LegacyRefId::inline_schema());
    assert!(ProfileRefId::inline_schema());
    assert!(LegacyRefAddress::inline_schema());
    assert!(ProfileRefAddress::inline_schema());
    assert_eq!(LegacyRefId::schema_name(), "LegacyRefId");
    assert_eq!(ProfileRefId::schema_id(), "ProfileRefId");
    assert_eq!(
        serde_json::to_value(LegacyRefId::parse("valid").unwrap()).unwrap(),
        "valid"
    );
}
#[test]
fn ref_siblings_and_scalar_compositions_are_checked_without_rewriting_constraints() {
    let mut generator = SchemaGenerator::default();
    let referenced = scalar_ref(&mut generator);
    let root = generator.root_schema_for::<String>();
    for node in [
        json_schema!({"$ref":referenced.get("$ref").unwrap(),"type":"object"}),
        json_schema!({"$ref":referenced.get("$ref").unwrap(),"type":"string","enum":["two scopes"]}),
        json_schema!({"anyOf":[{"type":"string"},{"type":"object"}]}),
        json_schema!({"type":["string","object"]}),
    ] {
        assert!(
            with_naming_profile(
                node,
                builtin(ScalarGrammar::ScopeToken),
                veoveo_types::NamingSchemaContext::new(&root)
            )
            .is_err()
        );
    }
    let node = json_schema!({"allOf":[referenced,{"maxLength":16}],"description":"same-instance restriction"});
    let admitted = with_naming_profile(
        node.clone(),
        builtin(ScalarGrammar::ScopeToken),
        veoveo_types::NamingSchemaContext::new(&root),
    )
    .unwrap();
    let mut plain = admitted.as_value().clone();
    plain.as_object_mut().unwrap().remove(NAMING_PROFILE_KEY);
    assert_eq!(plain, node.as_value().clone());
    let selected = with_naming_profile(
        json_schema!({"type":["string","null"]}),
        builtin(ScalarGrammar::ScopeToken),
        veoveo_types::NamingSchemaContext::new(&root),
    )
    .unwrap();
    assert!(
        naming_profile(&selected, veoveo_types::NamingSchemaContext::new(&root))
            .unwrap()
            .is_some()
    );
}

struct UnionAddresses;
impl ResourceProfile for UnionAddresses {
    type Error = ResourceUriError;
    const PROFILE: ResourceProfileSpec<Self::Error> = AddressProfiles::PROFILE;
    const SCHEMA: Option<ResourceSchema> = Some(ResourceSchema {
        schema: |_, g| scalar_union(g),
        inline: false,
    });
}
#[veoveo_types::resource_address(routes(UnionAddresses),schema=owner,wire)]
enum UnionAddress {
    #[resource(template = "independent://union")]
    Root,
}
#[veoveo_types::resource_address(custom(template="independent://array/{id}", error=ResourceUriError, route_error=|_| ResourceUriError::DisallowedComponent, schema=scalar_array))]
struct ArrayAddress {
    #[resource(variable="id",error=|_| ResourceUriError::DisallowedComponent)]
    id: PrincipalId,
}
#[test]
fn resource_callback_union_and_type_array_preserve_their_full_schema_graph() {
    for (actual, expected) in [
        (
            schemars::schema_for!(UnionAddress).as_value().clone(),
            raw_graph::<UnionAddress>(scalar_union),
        ),
        (
            schemars::schema_for!(ArrayAddress).as_value().clone(),
            raw_graph::<ArrayAddress>(scalar_array),
        ),
    ] {
        assert_eq!(naming_baseline::constraints(actual), expected);
    }
    assert!(!UnionAddress::inline_schema());
}
#[test]
fn referenced_owner_classification_is_preserved_and_conflicts_are_rejected() {
    let mut generator = SchemaGenerator::default();
    let schema = generator.subschema_for::<ScopeName>();
    assert_eq!(
        veoveo_types::naming::static_identity_schema(
            schema.clone(),
            None,
            "independent_owner",
            "ScopedId",
            &mut generator
        ),
        schema
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            veoveo_types::naming::static_identity_schema(
                schema,
                Some(ScalarNaming::builtin(ScalarGrammar::ScopeToken)),
                "independent_owner",
                "ScopedId",
                &mut generator,
            )
        }))
        .is_err()
    );
}

#[test]
fn actual_profile_schema_keeps_closed_revision_and_owner_reference_forms() {
    let root = schemars::schema_for!(NamingProfile);
    assert_eq!(root.get("type"), Some(&json!("object")));
    assert_eq!(root.get("additionalProperties"), Some(&json!(false)));
    assert_eq!(
        root.as_value()["properties"]["revision"]["minimum"],
        json!(1)
    );
    assert_eq!(
        root.as_value()["properties"]["revision"]["maximum"],
        json!(1)
    );
    assert_eq!(root.as_value()["required"], json!(["revision", "role"]));
    let mut wrong = declaration();
    wrong.authority = NamingAuthority::Standard {
        document: HttpsUrl::parse("https://www.rfc-editor.org/rfc/rfc9562").unwrap(),
    };
    assert!(
        NamingProfile::new(NamingRole::Scalar {
            profile: ScalarNaming::OwnerIdentity {
                declaration: wrong.clone()
            },
        })
        .is_err()
    );
    assert!(
        NamingProfile::new(NamingRole::Scalar {
            profile: ScalarNaming::Standard { declaration: wrong },
        })
        .is_ok()
    );
    assert!(
        NamingProfile::new(NamingRole::Scalar {
            profile: ScalarNaming::Standard {
                declaration: declaration()
            },
        })
        .is_err()
    );
}

#[path = "support/naming_repair.rs"]
mod naming_repair;
