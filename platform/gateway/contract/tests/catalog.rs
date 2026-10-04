use schemars::{JsonSchema, SchemaGenerator};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_gateway_contract::*;
use veoveo_types::{ExtensionError, ExtensionName};
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum InventedAction {
    Inspect,
}
#[derive(Serialize, serde::Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum InventedTarget {
    Foo,
    Bar,
}
fn audit(_: &InventedTarget) -> Result<TargetAuditResource, ExtensionError> {
    Ok(TargetAuditResource {
        server: veoveo_types::ServerSlug::new("invented").unwrap(),
        uri: veoveo_types::ResourceUri::new("invented://items/root").unwrap(),
    })
}
fn name(value: &str) -> ExtensionName {
    ExtensionName::new(value).unwrap()
}
fn descriptor(kind: &str) -> ActionDescriptor {
    ActionDescriptor {
        access: ActionAccess::Read,
        target_kinds: BTreeSet::from([name(kind)]),
        selectors: [
            RuleSelector::Profiles,
            RuleSelector::ProtectedResources,
            RuleSelector::Servers,
            RuleSelector::Tools,
            RuleSelector::ResourceSchemes,
            RuleSelector::Prompts,
        ]
        .into_iter()
        .map(|selector| (selector, SelectorRequirement::Optional))
        .collect(),
        server: None,
    }
}
fn make_registry() -> CatalogRegistry {
    let mut builder = CatalogRegistryBuilder::new(["core".into()], [name("gateway")]);
    builder
        .register_actions(vec![(InventedAction::Inspect, descriptor("foo"))])
        .unwrap();
    builder
        .register_target::<InventedTarget>(name("invented"), vec![name("foo")], audit)
        .unwrap();
    builder.build().unwrap()
}
#[test]
fn target_binding_and_schema_reject_unregistered_subset_and_foreign_identity() {
    let registry = make_registry();
    let key = registry
        .target_key::<InventedTarget>(&name("invented"))
        .unwrap();
    let target = registry
        .contribute_target(&key, &InventedTarget::Foo)
        .unwrap();
    registry.check_target(&target).unwrap();
    assert!(
        registry
            .contribute_target(&key, &InventedTarget::Bar)
            .is_err()
    );
    assert!(
        registry
            .admit_target(serde_json::json!({"kind":"bar"}))
            .is_err()
    );
    assert!(make_registry().check_target(&target).is_err());
    let schemas = registry.target_schemas(&mut SchemaGenerator::default());
    let validator = jsonschema::validator_for(schemas[0].as_value()).unwrap();
    assert!(validator.is_valid(&serde_json::json!({"kind":"foo"})));
    assert!(!validator.is_valid(&serde_json::json!({"kind":"bar"})));
    assert!(!validator.is_valid(&serde_json::json!({"kind":"foo","extra":true})));
    let actions = registry.action_schema();
    assert!(jsonschema::is_valid(
        actions.as_value(),
        &serde_json::json!("inspect")
    ));
    assert!(!jsonschema::is_valid(
        actions.as_value(),
        &serde_json::json!("unregistered")
    ));
}
#[test]
fn descriptors_require_complete_selectors_and_known_targets() {
    let mut builder = CatalogRegistryBuilder::new(Vec::<String>::new(), [name("gateway")]);
    let mut incomplete = descriptor("gateway");
    incomplete.selectors.remove(&RuleSelector::Profiles);
    assert!(
        builder
            .register_actions(vec![(InventedAction::Inspect, incomplete)])
            .is_err()
    );
    builder
        .register_actions(vec![(InventedAction::Inspect, descriptor("unknown"))])
        .unwrap();
    assert!(builder.build().is_err());
    let mut builder = CatalogRegistryBuilder::new(Vec::<String>::new(), [name("gateway")]);
    assert!(
        builder
            .register_target::<InventedTarget>(name("owner"), vec![name("gateway")], audit)
            .is_err()
    );
}
#[derive(Serialize, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Section {
    count: u32,
}
impl CatalogSection for Section {
    fn validate(&self, _: &CatalogFacts) -> Result<(), ExtensionError> {
        if self.count == 0 {
            Err(ExtensionError::new("count must be positive"))
        } else {
            Ok(())
        }
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        Vec::new()
    }
}
#[test]
fn section_admission_is_strict_checked_and_registry_bound() {
    let mut builder = CatalogRegistryBuilder::new(["core".into()], Vec::<ExtensionName>::new());
    assert!(builder.register_section::<Section>(name("core")).is_err());
    let key = builder
        .register_section::<Section>(name("invented"))
        .unwrap();
    assert!(
        builder
            .register_section::<Section>(name("invented"))
            .is_err()
    );
    let registry = builder.build().unwrap();
    let facts = CatalogFacts::default();
    assert!(
        registry
            .admit_sections(
                &BTreeMap::from([("unknown".into(), serde_json::json!({}))]),
                &facts
            )
            .is_err()
    );
    assert!(
        registry
            .admit_sections(
                &BTreeMap::from([("invented".into(), serde_json::json!({"count":0}))]),
                &facts
            )
            .is_err()
    );
    let sections = registry
        .admit_sections(
            &BTreeMap::from([("invented".into(), serde_json::json!({"count":1}))]),
            &facts,
        )
        .unwrap();
    assert_eq!(sections.get(&key).unwrap().unwrap().count, 1);
    let mut other = CatalogRegistryBuilder::new(Vec::<String>::new(), Vec::<ExtensionName>::new());
    let foreign = other.register_section::<Section>(name("invented")).unwrap();
    assert!(sections.get(&foreign).is_err());
}

#[derive(Serialize, serde::Deserialize, JsonSchema)]
struct ProjectedObjects(Vec<ProjectedObject>);
#[derive(Serialize, serde::Deserialize, JsonSchema)]
struct ProjectedObject {
    tenant: Option<veoveo_types::TenantId>,
    kind: String,
    id: String,
}
impl CatalogSection for ProjectedObjects {
    fn validate(&self, _: &CatalogFacts) -> Result<(), ExtensionError> {
        for item in &self.0 {
            ExtensionName::new(item.kind.clone())?;
        }
        Ok(())
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        Vec::new()
    }
    fn objects(&self) -> Result<Vec<CatalogObjectDescriptor>, ExtensionError> {
        self.0
            .iter()
            .map(|item| {
                Ok(CatalogObjectDescriptor {
                    tenant: item.tenant.clone(),
                    kind: ExtensionName::new(item.kind.clone())?,
                    id: item.id.clone(),
                    value: serde_json::json!({"value":1}),
                })
            })
            .collect()
    }
}
#[test]
fn contributed_object_identity_matches_persistence_and_cannot_replace_core() {
    let mut builder =
        CatalogRegistryBuilder::new(Vec::<String>::new(), Vec::<ExtensionName>::new());
    builder
        .register_section::<ProjectedObjects>(name("objects"))
        .unwrap();
    let registry = builder.build().unwrap();
    let admit = |items: serde_json::Value| {
        registry.admit_sections(
            &BTreeMap::from([("objects".into(), items)]),
            &CatalogFacts::default(),
        )
    };
    assert!(
        admit(serde_json::json!([
            {"tenant":"tenant-a","kind":"invented","id":"same"},
            {"tenant":"tenant-b","kind":"invented","id":"same"}
        ]))
        .is_err()
    );
    for kind in CORE_CATALOG_OBJECT_KINDS {
        assert!(
            admit(serde_json::json!([
                {"tenant":null,"kind":kind,"id":"owned"}
            ]))
            .is_err(),
            "core kind {kind} must be reserved"
        );
    }
    assert!(
        admit(serde_json::json!([
            {"tenant":"tenant-a","kind":"invented","id":"one"},
            {"tenant":"tenant-b","kind":"invented","id":"two"}
        ]))
        .is_ok()
    );
}
