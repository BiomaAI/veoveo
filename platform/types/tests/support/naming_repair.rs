use super::*;
use schemars::generate::SchemaSettings;
use std::{
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn dictionary(key: Schema) -> NamingProfile {
    NamingProfile::new(NamingRole::Dictionary { key_schema: key }).unwrap()
}
#[test]
fn dictionary_counterexamples_refuse_closed_dtos_extra_routes_and_malformed_keys() {
    for (map, key) in [
        (
            json_schema!({"type":"object","properties":{"optional_field":{"type":"string"}},"additionalProperties":false}),
            json_schema!({"type":"string"}),
        ),
        (
            json_schema!({"type":"object","properties":{"a":{"type":"string"}},"patternProperties":{".*":{"type":"string"}},"additionalProperties":false}),
            json_schema!({"type":"string","enum":["a"]}),
        ),
        (
            json_schema!({"type":"object","additionalProperties":true}),
            json_schema!({"type":"string","enum":42}),
        ),
        (
            json_schema!({"type":"object","additionalProperties":true}),
            json_schema!({"type":"string","pattern":42}),
        ),
        (
            json_schema!({"type":"object","properties":{"a":{"type":"string"}},"propertyNames":{"enum":["b"]},"additionalProperties":false}),
            json_schema!({"type":"string","enum":["a"]}),
        ),
        (
            json_schema!({"$ref":"#/$defs/Missing","type":"object","additionalProperties":true}),
            json_schema!({"type":"string"}),
        ),
    ] {
        assert!(
            with_naming_profile(
                map.clone(),
                dictionary(key),
                veoveo_types::NamingSchemaContext::new(&map)
            )
            .is_err()
        );
    }
    let heterogeneous = json_schema!({"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"additionalProperties":false});
    assert!(
        with_naming_profile(
            heterogeneous.clone(),
            dictionary(json_schema!({"type":"string","enum":["a","b"]})),
            veoveo_types::NamingSchemaContext::new(&heterogeneous)
        )
        .is_err()
    );
    let restricted = json_schema!({"type":"object","patternProperties":{"^[a-z]+$":{"type":"integer"}},"propertyNames":{"type":"string","maxLength":1},"additionalProperties":false});
    assert!(
        with_naming_profile(
            restricted.clone(),
            dictionary(json_schema!({"type":"string","pattern":"^[a-z]+$","maxLength":32})),
            veoveo_types::NamingSchemaContext::new(&restricted)
        )
        .is_err()
    );
    for marker in [
        json!({"revision":2,"role":{"kind":"scalar","profile":{"kind":"builtin","grammar":"scope_name"}}}),
        json!({"revision":1,"role":{"kind":"scalar","profile":{"kind":"builtin","grammar":"unknown"}}}),
    ] {
        let key = Schema::try_from(json!({"type":"string", (NAMING_PROFILE_KEY):marker})).unwrap();
        let map = json_schema!({"type":"object","additionalProperties":true});
        assert!(
            with_naming_profile(
                map.clone(),
                dictionary(key),
                veoveo_types::NamingSchemaContext::new(&map)
            )
            .is_err()
        );
    }
    let map = json_schema!({"type":"object","additionalProperties":true});
    with_naming_profile(
        map.clone(),
        dictionary(json_schema!({"type":"string"})),
        veoveo_types::NamingSchemaContext::new(&map),
    )
    .unwrap();
    let conflicting = with_naming_profile(
        map.clone(),
        NamingProfile::new(NamingRole::External {
            declaration: declaration(),
        })
        .unwrap(),
        veoveo_types::NamingSchemaContext::new(&map),
    )
    .unwrap();
    let root = json_schema!({"$defs":{"Map":conflicting}});
    assert!(
        with_naming_profile(
            json_schema!({"$ref":"#/$defs/Map"}),
            dictionary(json_schema!({"type":"string"})),
            veoveo_types::NamingSchemaContext::new(&root)
        )
        .is_err()
    );
}

struct MapProjection<K, const DECORATED: bool>(PhantomData<K>);
impl<K: JsonSchema, const DECORATED: bool> JsonSchema for MapProjection<K, DECORATED> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("IndependentMap_{}", K::schema_name()).into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        Self::schema_name()
    }
    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let map = <BTreeMap<K, Checked<Amount>>>::json_schema(g);
        if DECORATED {
            dictionary_schema::<K>(g, map).unwrap()
        } else {
            map
        }
    }
}
#[test]
fn actual_legacy_profile_pattern_key_maps_preserve_default_and_draft7_graphs() {
    fn actual<K: JsonSchema>() {
        for settings in [SchemaSettings::draft2020_12(), SchemaSettings::draft07()] {
            let marked = settings
                .clone()
                .into_generator()
                .into_root_schema_for::<MapProjection<K, true>>();
            let plain = settings
                .into_generator()
                .into_root_schema_for::<MapProjection<K, false>>();
            assert!(
                naming_profile(&marked, veoveo_types::NamingSchemaContext::new(&marked))
                    .unwrap()
                    .is_some()
            );
            assert_eq!(
                naming_baseline::constraints(marked.as_value().clone()),
                naming_baseline::constraints(plain.as_value().clone())
            );
            assert_eq!(
                marked.as_value()["patternProperties"]["^[a-z]+$"],
                plain.as_value()["patternProperties"]["^[a-z]+$"]
            );
            let NamingRole::Dictionary { key_schema } =
                naming_profile(&marked, veoveo_types::NamingSchemaContext::new(&marked))
                    .unwrap()
                    .unwrap()
                    .role()
                    .clone()
            else {
                panic!("key role")
            };
            assert!(key_schema.get("$ref").is_some() || key_schema.get("allOf").is_some());
        }
    }
    actual::<LegacyRefId>();
    actual::<ProfileRefId>();
    let mut generator = SchemaGenerator::default();
    let map = <BTreeMap<String, Value>>::json_schema(&mut generator);
    let marked = dictionary_schema::<String>(&mut generator, map.clone()).unwrap();
    assert_eq!(
        marked.get("additionalProperties"),
        map.get("additionalProperties")
    );
    assert_eq!(map.get("additionalProperties"), Some(&json!(true)));
}
#[test]
fn supported_dictionary_object_refs_and_draft7_allof_keep_constraints_and_key_markers() {
    let map = json_schema!({"type":"object","patternProperties":{"^[a-z]+$":{"type":"integer","minimum":1}},"additionalProperties":false});
    let key = with_naming_profile(
        json_schema!({"type":"string","pattern":"^[a-z]+$","minLength":1,"maxLength":32}),
        builtin(ScalarGrammar::ScopeName),
        veoveo_types::NamingSchemaContext::new(&json_schema!({})),
    )
    .unwrap();
    let root = json_schema!({"$defs":{"Map":map,"Key":key}});
    for key in [
        json_schema!({"$ref":"#/$defs/Key","description":"nominal key"}),
        json_schema!({"allOf":[{"$ref":"#/$defs/Key"}],"description":"Draft7 ref sibling"}),
    ] {
        for map in [
            json_schema!({"$ref":"#/$defs/Map","description":"map use site"}),
            json_schema!({"allOf":[{"$ref":"#/$defs/Map"}],"description":"map use site"}),
        ] {
            let marked = with_naming_profile(
                map.clone(),
                dictionary(key.clone()),
                veoveo_types::NamingSchemaContext::new(&root),
            )
            .unwrap();
            let mut constraints = marked.as_value().clone();
            constraints
                .as_object_mut()
                .unwrap()
                .remove(NAMING_PROFILE_KEY);
            assert_eq!(constraints, map.as_value().clone());
        }
    }
    let conflicting = json_schema!({"$ref":"#/$defs/Key", (NAMING_PROFILE_KEY): builtin(ScalarGrammar::TaskTypeName)});
    assert!(
        with_naming_profile(
            map.clone(),
            dictionary(conflicting),
            veoveo_types::NamingSchemaContext::new(&root)
        )
        .is_err()
    );
    for key in [
        json_schema!({"$ref":"https://external.example/key","type":"string"}),
        json_schema!({"$ref":"#/$defs/Missing","type":"string"}),
        json_schema!({"$ref":"#/$defs/Cycle","type":"string"}),
    ] {
        let root = json_schema!({"$defs":{"Cycle":{"$ref":"#/$defs/Cycle"}}});
        assert!(
            with_naming_profile(
                map.clone(),
                dictionary(key),
                veoveo_types::NamingSchemaContext::new(&root)
            )
            .is_err()
        );
    }
}
fn dynamic_callback(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type":"string","$dynamicRef":"#/$defs/Missing"})
}
#[veoveo_types::id(custom(error=&'static str,validate=admit_id,string,schema=dynamic_callback))]
struct DynamicCallbackId(String);
struct DynamicIds;
impl IdProfile for DynamicIds {
    type Error = &'static str;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|s, _| admit_id(s)).owner_schema(|g, _| dynamic_callback(g), true);
}
#[veoveo_types::id(text(DynamicIds))]
struct DynamicProfileId(String);
#[veoveo_types::resource_address(custom(template="independent://dynamic/{id}",error=ResourceUriError,route_error=|_| ResourceUriError::DisallowedComponent,schema=dynamic_callback,schema_inline))]
struct DynamicLegacyAddress {
    #[resource(variable="id",error=|_| ResourceUriError::DisallowedComponent)]
    id: PrincipalId,
}
struct DynamicAddressProfiles;
impl ResourceProfile for DynamicAddressProfiles {
    type Error = ResourceUriError;
    const PROFILE: ResourceProfileSpec<Self::Error> = AddressProfiles::PROFILE;
    const SCHEMA: Option<ResourceSchema> = Some(ResourceSchema {
        schema: |_, g| dynamic_callback(g),
        inline: true,
    });
}
#[veoveo_types::resource_address(routes(DynamicAddressProfiles),schema=owner,wire)]
enum DynamicProfileAddress {
    #[resource(template = "independent://dynamic")]
    Root,
}
#[test]
fn unsupported_dynamic_and_recursive_references_refuse_all_local_roles_and_callbacks() {
    for keyword in [
        "$dynamicRef",
        "$recursiveRef",
        "$dynamicAnchor",
        "$recursiveAnchor",
        "$anchor",
    ] {
        for reference in [
            "https://external.example/schema",
            "#/$defs/Missing",
            "#/$defs/Cycle",
        ] {
            let mut scalar = json_schema!({"type":"string"});
            scalar.insert(keyword.into(), json!(reference));
            assert!(
                scalar_schema(
                    scalar.clone(),
                    ScalarNaming::builtin(ScalarGrammar::ScopeName)
                )
                .is_err()
            );
            let mut object = json_schema!({"type":"object","additionalProperties":true});
            object.insert(keyword.into(), json!(reference));
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
                assert!(
                    with_naming_profile(
                        object.clone(),
                        NamingProfile::new(role).unwrap(),
                        veoveo_types::NamingSchemaContext::new(&object)
                    )
                    .is_err()
                );
            }
            let map = json_schema!({"type":"object","additionalProperties":true});
            assert!(
                with_naming_profile(
                    map.clone(),
                    dictionary(scalar),
                    veoveo_types::NamingSchemaContext::new(&map)
                )
                .is_err()
            );
        }
    }
    assert!(std::panic::catch_unwind(|| schemars::schema_for!(DynamicCallbackId)).is_err());
    assert!(std::panic::catch_unwind(|| schemars::schema_for!(DynamicProfileId)).is_err());
    assert!(std::panic::catch_unwind(|| schemars::schema_for!(DynamicLegacyAddress)).is_err());
    assert!(std::panic::catch_unwind(|| schemars::schema_for!(DynamicProfileAddress)).is_err());
}
#[test]
fn annotation_inspection_preserves_stateful_transform_count_and_constraint_graph() {
    fn generated<T: JsonSchema>(settings: SchemaSettings) -> (Value, usize) {
        let counter = Arc::new(AtomicUsize::new(0));
        let captured = counter.clone();
        let settings = settings.with_transform(move |schema: &mut Schema| {
            let n = captured.fetch_add(1, Ordering::SeqCst) + 1;
            schema.insert("x-owner-transform-pass".into(), json!(n));
        });
        let root = settings.into_generator().into_root_schema_for::<T>();
        (
            naming_baseline::constraints(root.as_value().clone()),
            counter.load(Ordering::SeqCst),
        )
    }
    for settings in [SchemaSettings::draft2020_12(), SchemaSettings::draft07()] {
        let (plain, plain_count) =
            generated::<MapProjection<ProfileRefId, false>>(settings.clone());
        let (marked, marked_count) =
            generated::<MapProjection<ProfileRefId, true>>(settings.clone());
        assert_eq!(plain_count, 1);
        assert_eq!(marked_count, 1);
        assert_eq!(plain, marked);
        let (_, id_count) = generated::<LegacyRefId>(settings.clone());
        let (_, profile_count) = generated::<ProfileRefId>(settings.clone());
        let (_, address_count) = generated::<LegacyRefAddress>(settings.clone());
        let (_, profile_address_count) = generated::<ProfileRefAddress>(settings);
        assert_eq!(
            [
                id_count,
                profile_count,
                address_count,
                profile_address_count
            ],
            [1; 4]
        );
    }
}

static KEY_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static CAPTURED_KEY: std::sync::Mutex<Option<Schema>> = std::sync::Mutex::new(None);
struct StatefulKey;
impl JsonSchema for StatefulKey {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "IndependentStatefulKey".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let request = KEY_REQUESTS.fetch_add(1, Ordering::SeqCst);
        let key = json_schema!({"type":"string","pattern":format!("^key_{request}$")});
        *CAPTURED_KEY.lock().unwrap() = Some(key.clone());
        key
    }
}
#[test]
fn pure_captured_key_decoration_has_no_callbacks_or_transforms_and_convenience_refuses_drift() {
    KEY_REQUESTS.store(0, Ordering::SeqCst);
    let transformations = Arc::new(AtomicUsize::new(0));
    let count = transformations.clone();
    let settings = SchemaSettings::draft2020_12().with_transform(move |_: &mut Schema| {
        count.fetch_add(1, Ordering::SeqCst);
    });
    let mut generator = settings.into_generator();
    let map = <BTreeMap<StatefulKey, Checked<Amount>>>::json_schema(&mut generator);
    let key = CAPTURED_KEY.lock().unwrap().clone().unwrap();
    let definitions = generator.definitions().clone();
    assert_eq!(KEY_REQUESTS.load(Ordering::SeqCst), 1);
    let marked = dictionary_schema_with_key(&generator, map.clone(), key).unwrap();
    assert_eq!(KEY_REQUESTS.load(Ordering::SeqCst), 1);
    assert_eq!(transformations.load(Ordering::SeqCst), 0);
    assert_eq!(generator.definitions(), &definitions);
    let mut plain = marked.as_value().clone();
    plain.as_object_mut().unwrap().remove(NAMING_PROFILE_KEY);
    assert_eq!(plain, map.as_value().clone());
    assert!(dictionary_schema::<StatefulKey>(&mut generator, map).is_err());
    assert_eq!(KEY_REQUESTS.load(Ordering::SeqCst), 2);
    assert_eq!(transformations.load(Ordering::SeqCst), 0);
}

trait RawProjection: JsonSchema {
    fn raw(generator: &mut SchemaGenerator) -> Schema;
}
impl RawProjection for LegacyRefId {
    fn raw(g: &mut SchemaGenerator) -> Schema {
        scalar_ref(g)
    }
}
impl RawProjection for ProfileRefId {
    fn raw(g: &mut SchemaGenerator) -> Schema {
        scalar_ref(g)
    }
}
impl RawProjection for LegacyRefAddress {
    fn raw(g: &mut SchemaGenerator) -> Schema {
        scalar_ref(g)
    }
}
impl RawProjection for ProfileRefAddress {
    fn raw(g: &mut SchemaGenerator) -> Schema {
        scalar_ref(g)
    }
}
struct RawCallback<T>(PhantomData<T>);
impl<T: RawProjection> JsonSchema for RawCallback<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        T::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        T::schema_id()
    }
    fn inline_schema() -> bool {
        T::inline_schema()
    }
    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        T::raw(g)
    }
}
#[test]
fn scalar_callback_root_constraints_match_baseline_with_one_stateful_transform() {
    fn graph<T: JsonSchema>(settings: SchemaSettings) -> (Value, usize) {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let settings = settings.with_transform(move |schema: &mut Schema| {
            let pass = observed.fetch_add(1, Ordering::SeqCst) + 1;
            schema.insert("maxLength".into(), json!(64 - pass));
        });
        let schema = settings.into_generator().into_root_schema_for::<T>();
        (
            naming_baseline::constraints(schema.as_value().clone()),
            count.load(Ordering::SeqCst),
        )
    }
    fn actual<T: RawProjection>() {
        for settings in [SchemaSettings::draft2020_12(), SchemaSettings::draft07()] {
            let (plain, plain_count) = graph::<RawCallback<T>>(settings.clone());
            let (decorated, decorated_count) = graph::<T>(settings);
            assert_eq!(plain_count, 1);
            assert_eq!(decorated_count, 1);
            assert_eq!(plain, decorated);
        }
    }
    actual::<LegacyRefId>();
    actual::<ProfileRefId>();
    actual::<LegacyRefAddress>();
    actual::<ProfileRefAddress>();
}
#[test]
fn configured_nested_escaped_definitions_path_is_inspected_without_generating_a_root() {
    let mut settings = SchemaSettings::draft2020_12();
    settings.definitions_path = "#/components/owner~1schemas/".into();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let settings = settings.with_transform(move |_: &mut Schema| {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    let mut generator = settings.into_generator();
    let schema = scalar_ref(&mut generator);
    let definitions = generator.definitions().clone();
    let _classified = veoveo_types::naming::static_identity_schema(
        schema,
        None,
        "independent_owner",
        "NestedScalar",
        &mut generator,
    );
    assert_eq!(generator.definitions(), &definitions);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn convenience_key_definition_is_explicit_annotation_support_and_existing_definitions_are_unchanged()
 {
    let mut generator = SchemaGenerator::default();
    let map = <BTreeMap<Metric, Checked<Amount>>>::json_schema(&mut generator);
    let existing = generator.definitions().clone();
    let marked = dictionary_schema::<Metric>(&mut generator, map.clone()).unwrap();
    for (name, body) in &existing {
        assert_eq!(generator.definitions().get(name), Some(body));
    }
    let additions: Vec<_> = generator
        .definitions()
        .keys()
        .filter(|name| !existing.contains_key(*name))
        .collect();
    assert_eq!(additions.len(), 1);
    let profile = marked.get(NAMING_PROFILE_KEY).unwrap();
    let reference = profile["role"]["keySchema"]["$ref"].as_str().unwrap();
    let root = generator.into_root_schema_for::<BTreeMap<Metric, Checked<Amount>>>();
    assert_eq!(
        root.as_value()
            .pointer(reference.strip_prefix('#').unwrap())
            .unwrap()["enum"],
        json!(["travel_time", "route_variance"])
    );
    let mut body = marked.as_value().clone();
    body.as_object_mut().unwrap().remove(NAMING_PROFILE_KEY);
    assert_eq!(body, map.as_value().clone());
}

#[test]
fn rebased_map_and_key_references_refuse_without_borrowing_export_root_definitions() {
    let map = json_schema!({"type":"object","properties":{"a":{"type":"integer"}},"additionalProperties":false});
    let key = json_schema!({"type":"string","enum":["a"]});
    let root = json_schema!({"$id":"https://owner.example/export","$defs":{"Map":map,"Key":key}});
    let map_use = json_schema!({"$ref":"#/$defs/Map"});
    let key_use = json_schema!({"$ref":"#/$defs/Key"});
    with_naming_profile(
        map_use.clone(),
        dictionary(key_use.clone()),
        veoveo_types::NamingSchemaContext::new(&root),
    )
    .unwrap();
    let exported_map = json_schema!({"$id":"https://owner.example/export","type":"object",
        "properties":{"a":{"type":"integer"}},"additionalProperties":false,"$defs":{"Key":key}});
    with_naming_profile(
        exported_map.clone(),
        dictionary(key_use.clone()),
        veoveo_types::NamingSchemaContext::new(&exported_map),
    )
    .unwrap();
    let rebased_map = json_schema!({"$id":"https://owner.example/other","$ref":"#/$defs/Map"});
    assert!(
        with_naming_profile(
            rebased_map,
            dictionary(key_use.clone()),
            veoveo_types::NamingSchemaContext::new(&root)
        )
        .is_err()
    );
    let rebased_key = json_schema!({"$id":"https://owner.example/key","$ref":"#/$defs/Key"});
    let key_root = json_schema!({"$id":"https://owner.example/export","$defs":{"Map":map,"Key":key,"RebasedKey":rebased_key}});
    assert!(
        with_naming_profile(
            map_use,
            dictionary(json_schema!({"$ref":"#/$defs/RebasedKey"})),
            veoveo_types::NamingSchemaContext::new(&key_root)
        )
        .is_err()
    );
    let ancestor = json_schema!({"$defs":{"Scoped":{"$id":"https://owner.example/nested","$defs":{"Key":key}}}});
    assert!(
        with_naming_profile(
            map,
            dictionary(json_schema!({"$ref":"#/$defs/Scoped/$defs/Key"})),
            veoveo_types::NamingSchemaContext::new(&ancestor)
        )
        .is_err()
    );
}

#[test]
fn definition_and_property_names_do_not_rebase_local_schema_references() {
    let key = json_schema!({"type":"string","enum":["id","$id"]});
    let map = json_schema!({"type":"object","properties":{"id":{"type":"integer"},"$id":{"type":"integer"}},"additionalProperties":false});
    let root = json_schema!({"$defs":{"id":{"type":"string"},"$id":{"type":"string"},"Map":map,"Key":key}});
    with_naming_profile(
        json_schema!({"$ref":"#/$defs/Map"}),
        dictionary(json_schema!({"$ref":"#/$defs/Key"})),
        veoveo_types::NamingSchemaContext::new(&root),
    )
    .unwrap();
    let named_root = json_schema!({"$defs":{"id":map,"$id":key}});
    with_naming_profile(
        json_schema!({"$ref":"#/$defs/id"}),
        dictionary(json_schema!({"$ref":"#/$defs/$id"})),
        veoveo_types::NamingSchemaContext::new(&named_root),
    )
    .unwrap();
}

#[test]
fn actual_generator_definitions_context_preserves_named_entries_and_refuses_rebased_schemas() {
    let mut custom = SchemaSettings::draft2020_12();
    custom.definitions_path = "#/components/owner~1schemas/".into();
    let mut named_prefix = SchemaSettings::openapi3();
    named_prefix.definitions_path = "/components/id/$id/schemas".into();
    for settings in [
        SchemaSettings::draft2020_12(),
        SchemaSettings::draft07(),
        SchemaSettings::openapi3(),
        custom,
        named_prefix,
    ] {
        let mut generator = settings.clone().into_generator();
        generator
            .definitions_mut()
            .insert("id".into(), json!({"type":"string"}));
        generator
            .definitions_mut()
            .insert("$id".into(), json!({"type":"string"}));
        let map = <BTreeMap<Metric, Checked<Amount>>>::json_schema(&mut generator);
        let key = generator.subschema_for::<Metric>();
        generator
            .definitions_mut()
            .insert("Map".into(), map.as_value().clone());
        let before = generator.definitions().clone();
        let marked = dictionary_schema_with_key(&generator, map.clone(), key.clone()).unwrap();
        assert_eq!(generator.definitions(), &before);
        let mut unmarked = marked.clone();
        unmarked.ensure_object().remove(NAMING_PROFILE_KEY);
        assert_eq!(unmarked, map);
        let pointer = settings
            .definitions_path
            .strip_prefix('#')
            .unwrap_or(&settings.definitions_path)
            .trim_end_matches('/');
        let mut root = json!({});
        let mut container = &mut root;
        for segment in pointer.trim_start_matches('/').split('/') {
            let segment = segment.replace("~1", "/").replace("~0", "~");
            container = container
                .as_object_mut()
                .unwrap()
                .entry(segment)
                .or_insert_with(|| json!({}));
        }
        *container = json!(before);
        let exported = Schema::try_from(root).unwrap();
        let context = NamingSchemaContext::new(&exported)
            .with_definitions_path(&settings.definitions_path)
            .unwrap();
        assert!(naming_profile(&marked, context).unwrap().is_some());
        with_naming_profile(
            Schema::try_from(json!({"$ref":format!("#{pointer}/Map")})).unwrap(),
            dictionary(key),
            context,
        )
        .unwrap();
        // Captured owner exports use the same explicit location and may name entries id/$id.
        let mut retained = exported.as_value().clone();
        retained.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert("Scoped".into(),json!({"$id":"https://owner.example/rebased","type":"string","enum":["observedValue"]}));
        let retained = Schema::try_from(retained).unwrap();
        let context = NamingSchemaContext::new(&retained)
            .with_definitions_path(&settings.definitions_path)
            .unwrap();
        let reference = format!("#{pointer}/Scoped");
        assert!(
            with_naming_profile(
                map.clone(),
                dictionary(Schema::try_from(json!({"$ref":reference})).unwrap()),
                context
            )
            .is_err()
        );
    }
    let root = json_schema!({"$defs":{"Scoped":{"$id":"https://owner.example/other","$defs":{"Key":{"type":"string"}}}}});
    assert!(
        NamingSchemaContext::new(&root)
            .with_definitions_path("#/$defs/Scoped/$defs")
            .is_err()
    );
    for bad in [
        "https://owner.example/schema",
        "#/$defs/Absent",
        "#/$defs/Scoped/$id",
    ] {
        assert!(
            NamingSchemaContext::new(&root)
                .with_definitions_path(bad)
                .is_err()
        );
    }
}

#[test]
fn declared_context_class_matrix_distinguishes_containers_from_schema_scopes() {
    let map =
        json!({"type":"object","properties":{"a":{"type":"integer"}},"additionalProperties":false});
    let key = json!({"type":"string","enum":["a"]});
    for pointer in [
        "/$defs",
        "/definitions",
        "/components/schemas",
        "/components/owner~1schemas",
        "/components/id/$id/schemas",
    ] {
        let definitions =
            json!({"id":{"type":"string"},"$id":{"type":"string"},"Map":map,"Key":key});
        let mut root = json!({});
        let mut container = &mut root;
        for segment in pointer.trim_start_matches('/').split('/') {
            let segment = segment.replace("~1", "/").replace("~0", "~");
            container = container
                .as_object_mut()
                .unwrap()
                .entry(segment)
                .or_insert_with(|| json!({}));
        }
        *container = definitions;
        let root = Schema::try_from(root).unwrap();
        let context = NamingSchemaContext::new(&root)
            .with_definitions_path(pointer)
            .unwrap();
        let map_use = Schema::try_from(json!({"$ref":format!("#{pointer}/Map")})).unwrap();
        let key_use = Schema::try_from(json!({"$ref":format!("#{pointer}/Key")})).unwrap();
        with_naming_profile(map_use.clone(), dictionary(key_use.clone()), context).unwrap();
        let mut scoped_use = map_use.clone();
        scoped_use.insert("$id".into(), json!("https://owner.example/use"));
        assert!(with_naming_profile(scoped_use, dictionary(key_use.clone()), context).is_err());
        for target in ["Map", "Key"] {
            let mut scoped = root.as_value().clone();
            scoped.pointer_mut(pointer).unwrap()[target]["$id"] =
                json!("https://owner.example/member");
            let scoped = Schema::try_from(scoped).unwrap();
            let context = NamingSchemaContext::new(&scoped)
                .with_definitions_path(pointer)
                .unwrap();
            assert!(
                with_naming_profile(map_use.clone(), dictionary(key_use.clone()), context).is_err()
            );
        }
    }
    let properties = json_schema!({"properties":{"id":map,"$id":key}});
    with_naming_profile(
        json_schema!({"$ref":"#/properties/id"}),
        dictionary(json_schema!({"$ref":"#/properties/$id"})),
        NamingSchemaContext::new(&properties),
    )
    .unwrap();
    for root in [
        json_schema!({"properties":{"Widget":{"$id":"https://owner.example/other","$defs":{"Key":key}}}}),
        json_schema!({"$defs":{"Widget":{"$id":"https://owner.example/other","$defs":{"Key":key}}}}),
    ] {
        let pointer = if root.get("properties").is_some() {
            "/properties/Widget/$defs"
        } else {
            "/$defs/Widget/$defs"
        };
        assert!(
            NamingSchemaContext::new(&root)
                .with_definitions_path(pointer)
                .is_err()
        );
    }
    for (root, pointer) in [
        (json_schema!({"$defs":[]}), "/$defs"),
        (
            json_schema!({"$defs":{"Key":key}}),
            "https://owner.example/schema",
        ),
        (json_schema!({"$defs":{"Key":key}}), "/$defs/Key/type"),
        (json_schema!({"$defs":{"Key":key}}), "/$defs/Missing"),
    ] {
        assert!(
            NamingSchemaContext::new(&root)
                .with_definitions_path(pointer)
                .is_err()
        );
    }
}

struct EncodedSchemaKey<const N: u8>;
impl<const N: u8> JsonSchema for EncodedSchemaKey<N> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        match N {
            0 => "Key A",
            1 => "Ключ",
            2 => "Key%20A",
            _ => "Key~/A",
        }
        .into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        format!("independent::{}", Self::schema_name()).into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string","enum":["a"]})
    }
}
#[test]
fn maintained_generator_reference_encoding_binds_only_the_decoded_definition() {
    fn actual<const N: u8>() {
        let mut generator = SchemaSettings::draft2020_12().into_generator();
        let key = generator.subschema_for::<EncodedSchemaKey<N>>();
        let reference = key.get("$ref").unwrap().as_str().unwrap();
        assert!(reference.contains('%') || reference.contains("~0~1"));
        // The literal encoded spelling is a distinct decoy, not an alias for the key.
        let decoy = reference.strip_prefix("#/$defs/").unwrap();
        generator
            .definitions_mut()
            .insert(decoy.into(), json!({"type":"string","enum":["b"]}));
        let good = json_schema!({"type":"object","properties":{"a":{"type":"integer"}},"additionalProperties":false});
        let bad = json_schema!({"type":"object","properties":{"b":{"type":"integer"}},"additionalProperties":false});
        dictionary_schema_with_key(&generator, good, key.clone()).unwrap();
        assert!(dictionary_schema_with_key(&generator, bad, key.clone()).is_err());
        generator
            .definitions_mut()
            .get_mut(EncodedSchemaKey::<N>::schema_name().as_ref())
            .unwrap()["$id"] = json!("https://owner.example/rebased");
        assert!(dictionary_schema_with_key(&generator,json_schema!({"type":"object","properties":{"a":{"type":"integer"}},"additionalProperties":false}),key).is_err());
    }
    actual::<0>();
    actual::<1>();
    actual::<2>();
    actual::<3>();
    let root = json_schema!({"$defs":{"Key":{"type":"string","enum":["a"]},"Key~2":{"type":"string","enum":["a"]},"Key~":{"type":"string","enum":["a"]}}});
    let map = json_schema!({"type":"object","properties":{"a":{"type":"integer"}},"additionalProperties":false});
    for reference in [
        "#/$defs/Key%",
        "#/$defs/Key%2",
        "#/$defs/Key%GG",
        "#/$defs/%FF",
        "#/$defs/Key~2",
        "#/$defs/Key~",
        "#/$defs//Key",
        "#",
        "#/",
        "https://owner.example/schema#/$defs/Key",
    ] {
        assert!(
            with_naming_profile(
                map.clone(),
                dictionary(Schema::try_from(json!({"$ref":reference})).unwrap()),
                NamingSchemaContext::new(&root)
            )
            .is_err(),
            "admitted {reference}"
        );
    }
}
#[test]
fn generator_empty_pointer_segments_are_refused_without_binding_a_different_container() {
    for path in [
        "/components/schemas///",
        "/components//schemas",
        "/",
        "",
        "/components/owner~2schemas",
    ] {
        let mut settings = SchemaSettings::draft2020_12();
        settings.definitions_path = path.into();
        let generator = settings.into_generator();
        assert!(
            dictionary_schema_with_key(
                &generator,
                json_schema!({"type":"object","additionalProperties":{}}),
                json_schema!({"type":"string"})
            )
            .is_err(),
            "admitted {path}"
        );
    }
}
