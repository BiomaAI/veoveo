//! The protocol schema adapter composes only identity-resolved gateway models.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::Value;
use veoveo_gateway_contract::{CatalogRegistry, GatewayAction};

use super::{GatewayControlPlane, PolicyTarget};

/// Wire schema placeholder for admitted ActionName fields, distinct from the
/// genuinely kernel-only GatewayAction Rust type.
pub(crate) struct RegisteredActionSchema;
impl JsonSchema for RegisteredActionSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        GatewayAction::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::RegisteredActionSchema").into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        GatewayAction::json_schema(generator)
    }
}

pub fn composed_gateway_schema<T: JsonSchema>(registry: &CatalogRegistry) -> Schema {
    let mut generator = SchemaGenerator::default();
    let schema = generator.root_schema_for::<T>();
    let original_definitions = generator.definitions().clone();
    let action = model_location::<T, RegisteredActionSchema>(&mut generator, &schema);
    let target = model_location::<T, PolicyTarget>(&mut generator, &schema);
    let plane = model_location::<T, GatewayControlPlane>(&mut generator, &schema);
    // Probes keep the live identity/name map but their unused definitions must
    // never leak into the published document. Owner codecs regenerate any needed
    // definition through those same identity mappings.
    *generator.definitions_mut() = original_definitions;
    if action.is_none() && target.is_none() && plane.is_none() {
        return schema;
    }

    let mut value = schema.to_value();
    if let Some(location) = action {
        // Keep schema annotations; only the admitted string vocabulary changes.
        location
            .value_mut(&mut value)
            .as_object_mut()
            .expect("action schema object")
            .extend(
                registry
                    .action_schema()
                    .to_value()
                    .as_object()
                    .expect("action schema object")
                    .clone(),
            );
    }
    if let Some(location) = target {
        let targets = registry.target_schemas(&mut generator);
        location
            .value_mut(&mut value)
            .get_mut("anyOf")
            .and_then(Value::as_array_mut)
            .expect("PolicyTarget union")
            .extend(targets.into_iter().map(Schema::to_value));
    }
    if let Some(location) = plane {
        let sections = registry.section_schemas(&mut generator);
        let object = location
            .value_mut(&mut value)
            .as_object_mut()
            .expect("control plane schema object");
        let properties = object
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .expect("control plane properties");
        properties.extend(
            sections
                .into_iter()
                .map(|(name, schema)| (name, schema.to_value())),
        );
        object.insert("additionalProperties".into(), Value::Bool(false));
    }
    // Owner codecs use the same generator, including its collision-safe names.
    // Preserve definitions already composed above and add only new owner definitions.
    if !generator.definitions().is_empty() {
        let defs = value
            .as_object_mut()
            .expect("root schema object")
            .entry("$defs")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
            .expect("schema definitions object");
        for (name, definition) in generator.definitions() {
            defs.entry(name.clone())
                .or_insert_with(|| definition.clone());
        }
    }
    value.try_into().expect("composed schema object")
}

enum ModelLocation {
    Root,
    Definition(String),
}
impl ModelLocation {
    fn value_mut<'a>(&self, schema: &'a mut Value) -> &'a mut Value {
        match self {
            Self::Root => schema,
            Self::Definition(pointer) => schema
                .pointer_mut(pointer)
                .expect("resolved schema definition"),
        }
    }
}

fn model_location<Root: JsonSchema, Model: JsonSchema>(
    generator: &mut SchemaGenerator,
    schema: &Schema,
) -> Option<ModelLocation> {
    if Root::schema_id() == Model::schema_id() {
        return Some(ModelLocation::Root);
    }
    // Only the live generator retains schema_id-to-name identity. Clone resets
    // that map and could resolve a colliding owner definition as this model.
    let reference = generator.subschema_for::<Model>();
    let pointer = reference.get("$ref")?.as_str()?.strip_prefix('#')?;
    schema.as_value().pointer(pointer)?;
    Some(ModelLocation::Definition(pointer.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::json_schema;

    #[allow(dead_code)] // Schema-only collision fixture.
    #[derive(JsonSchema)]
    #[schemars(rename = "GatewayAction")]
    struct OwnerAction {
        owner_value: String,
    }
    #[allow(dead_code)] // Schema-only collision fixture.
    #[derive(JsonSchema)]
    #[schemars(rename = "PolicyTarget")]
    struct OwnerTarget {
        owner_value: String,
    }
    #[allow(dead_code)] // Schema-only collision fixture.
    #[derive(JsonSchema)]
    #[schemars(rename = "GatewayControlPlane")]
    struct OwnerPlane {
        owner_value: String,
    }

    struct Annotations;
    impl JsonSchema for Annotations {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "OwnerAnnotations".into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            json_schema!({"type":"object", "examples":[{"title":"GatewayAction","enum":["owner_only"],"nested":{"title":"PolicyTarget","oneOf":[{"const":"owner_only"}]}}], "default":{"title":"GatewayControlPlane","properties":{"owner_only":{"type":"string"}}}, "x-owner":{"definitions":{"GatewayAction":{"const":"owner_only"}}}})
        }
    }
    #[allow(dead_code)] // Schema-only fixture.
    #[derive(JsonSchema)]
    struct Unrelated {
        action: OwnerAction,
        target: OwnerTarget,
        plane: OwnerPlane,
        annotations: Annotations,
    }
    #[allow(dead_code)] // Schema-only fixture.
    #[derive(JsonSchema)]
    struct Mixed {
        owner_action: OwnerAction,
        owner_target: OwnerTarget,
        owner_plane: OwnerPlane,
        annotations: Annotations,
        rule: super::super::PolicyRule,
        kernel_action: GatewayAction,
        decision: super::super::PolicyDecision,
        plane: GatewayControlPlane,
    }
    fn property_definition<'a>(schema: &'a Value, property: &str) -> &'a Value {
        let property = &schema["properties"][property];
        match property.get("$ref").and_then(Value::as_str) {
            Some(reference) => schema
                .pointer(reference.strip_prefix('#').unwrap())
                .unwrap(),
            None => property,
        }
    }
    #[test]
    fn unrelated_owner_names_and_annotations_are_unchanged() {
        let registry = crate::catalog_fixture::fresh_registry();
        assert_eq!(
            composed_gateway_schema::<Unrelated>(&registry),
            schemars::schema_for!(Unrelated)
        );
        assert_eq!(
            composed_gateway_schema::<OwnerPlane>(&registry),
            schemars::schema_for!(OwnerPlane)
        );
        assert_eq!(
            composed_gateway_schema::<GatewayAction>(&registry),
            schemars::schema_for!(GatewayAction)
        );
        assert_eq!(
            composed_gateway_schema::<String>(&registry),
            schemars::schema_for!(String)
        );
    }
    #[test]
    fn colliding_definitions_are_preserved_alongside_real_gateway_models() {
        let registry = crate::catalog_fixture::fresh_registry();
        let original = schemars::schema_for!(Mixed).to_value();
        let composed = composed_gateway_schema::<Mixed>(&registry).to_value();
        for name in ["owner_action", "owner_target", "owner_plane", "annotations"] {
            assert_eq!(
                property_definition(&original, name),
                property_definition(&composed, name),
                "owner schema {name}"
            );
        }
        let rule = property_definition(&composed, "rule");
        let action_ref = rule
            .pointer("/properties/actions/items/$ref")
            .unwrap()
            .as_str()
            .unwrap();
        let actions = composed
            .pointer(action_ref.strip_prefix('#').unwrap())
            .unwrap()["enum"]
            .as_array()
            .unwrap();
        assert_eq!(actions.len(), registry.actions().names().count());
        assert!(actions.contains(&Value::from("recording_batch_append")));
        let kernel = property_definition(&composed, "kernel_action");
        let validator = jsonschema::validator_for(kernel).unwrap();
        assert!(validator.is_valid(&Value::from("tools_call")));
        assert!(!validator.is_valid(&Value::from("recording_batch_append")));
        assert_eq!(
            kernel["enum"].as_array().unwrap().len(),
            GatewayAction::ALL.len()
        );
        let plane = property_definition(&composed, "plane");
        assert!(
            plane["properties"]
                .get("recording_ingest_resources")
                .is_some()
        );
        assert_eq!(plane["additionalProperties"], false);
    }
    #[test]
    fn published_policy_and_plane_schemas_admit_registered_wire_values() {
        use super::super::{PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRule};
        use veoveo_recording_contract::{RECORDING_TARGET_GROUP, RecordingAction, RecordingTarget};
        let registry = crate::catalog_fixture::fresh_registry();
        let plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../../configs/gateway.smoke.json")).unwrap();
        plane.validate(&registry).unwrap();
        let schema = composed_gateway_schema::<GatewayControlPlane>(&registry);
        let validator = jsonschema::validator_for(schema.as_value()).unwrap();
        assert!(validator.is_valid(&serde_json::to_value(&plane).unwrap()));
        let mut rule = plane.policies[0].rules[0].clone();
        rule.actions = registry.actions().names().cloned().collect();
        let schema = composed_gateway_schema::<PolicyRule>(&registry);
        assert!(
            jsonschema::validator_for(schema.as_value())
                .unwrap()
                .is_valid(&serde_json::to_value(&rule).unwrap())
        );
        let target = RecordingTarget::RecordingProducer {
            producer: "sensor-a".parse().unwrap(),
        };
        let target = registry
            .contribute_target(
                &registry
                    .target_key::<RecordingTarget>(
                        &veoveo_types::ExtensionName::parse(RECORDING_TARGET_GROUP).unwrap(),
                    )
                    .unwrap(),
                &target,
            )
            .unwrap();
        let decision = PolicyDecision {
            effect: PolicyEffect::Allow,
            reason: PolicyReasonCode::PolicyAllow,
            evaluated_at: chrono::Utc::now(),
            profile: "workspace".parse().unwrap(),
            action: registry
                .action_key::<RecordingAction>()
                .unwrap()
                .action(RecordingAction::BatchAppend)
                .unwrap()
                .name()
                .clone(),
            target: PolicyTarget::Owner(target),
            principal: None,
            tenant: None,
            policy_version: None,
            rule_id: None,
            trace_id: "schema-regression".parse().unwrap(),
        };
        let schema = composed_gateway_schema::<PolicyDecision>(&registry);
        assert!(
            jsonschema::validator_for(schema.as_value())
                .unwrap()
                .is_valid(&serde_json::to_value(&decision).unwrap())
        );
    }
}
