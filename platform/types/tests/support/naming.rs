//! Constraint baseline comparisons admit each new marker before removing only that metadata.
pub fn constraints(mut value: serde_json::Value) -> serde_json::Value {
    let root = schemars::Schema::try_from(value.clone()).unwrap();
    fn visit(value: &mut serde_json::Value, root: &schemars::Schema) {
        match value {
            serde_json::Value::Object(map) => {
                if map.contains_key(veoveo_types::NAMING_PROFILE_KEY) {
                    let node =
                        schemars::Schema::try_from(serde_json::Value::Object(map.clone())).unwrap();
                    assert!(
                        veoveo_types::naming_profile(
                            &node,
                            veoveo_types::NamingSchemaContext::new(root)
                        )
                        .unwrap()
                        .is_some()
                    );
                    map.remove(veoveo_types::NAMING_PROFILE_KEY);
                }
                for child in map.values_mut() {
                    visit(child, root);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    visit(child, root);
                }
            }
            _ => {}
        }
    }
    visit(&mut value, &root);
    value
}
