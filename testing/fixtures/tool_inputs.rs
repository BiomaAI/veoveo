//! Pure input fixtures shared by isolated consumers and existing hosted owner tests.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInputCase {
    pub branch: String,
    pub tool: String,
    pub arguments: Value,
    tag: String,
    required: Vec<String>,
    defaults: BTreeMap<String, Value>,
    open_objects: Vec<String>,
}

impl ToolInputCase {
    pub fn load(bytes: &[u8]) -> Vec<Self> {
        let cases: Vec<Self> = serde_json::from_slice(bytes).expect("controlled input fixture");
        let branches: BTreeSet<_> = cases.iter().map(|case| case.branch.as_str()).collect();
        assert_eq!(branches.len(), cases.len(), "repeated fixture branch");
        for case in &cases {
            let (_, variant) = case.branch.rsplit_once('.').expect("named branch variant");
            assert_eq!(
                case.arguments.pointer(&case.tag).and_then(Value::as_str),
                Some(variant),
                "{} tag does not match its label",
                case.branch
            );
            for pointer in &case.open_objects {
                assert!(
                    case.arguments
                        .pointer(pointer)
                        .is_some_and(Value::is_object),
                    "{} missing open map at {pointer}",
                    case.branch
                );
            }
            for pointer in case.defaults.keys() {
                assert!(
                    case.arguments.pointer(pointer).is_none(),
                    "{} explicitly supplies default at {pointer}",
                    case.branch
                );
            }
        }
        cases
    }

    pub fn decode<T: DeserializeOwned + Serialize>(&self) -> T {
        let decoded: T = serde_json::from_slice(&serde_json::to_vec(&self.arguments).unwrap())
            .unwrap_or_else(|error| panic!("{} baseline: {error}", self.branch));
        for value in self.open_values() {
            serde_json::from_slice::<T>(&serde_json::to_vec(&value).unwrap())
                .unwrap_or_else(|error| panic!("{} open map: {error}", self.branch));
        }
        let serialized = serde_json::to_value(&decoded).unwrap();
        for (pointer, expected) in &self.defaults {
            assert_eq!(
                serialized.pointer(pointer),
                Some(expected),
                "{} default at {pointer}",
                self.branch
            );
        }
        decoded
    }

    /// Only fixture-declared open maps escape traversal; all other actual objects
    /// receive an unknown field, including each object inside an array.
    pub fn unknown_fields(&self) -> Vec<(String, Value)> {
        let mut objects = Vec::new();
        self.objects(&self.arguments, String::new(), &mut objects);
        objects
            .into_iter()
            .map(|pointer| {
                let mut value = self.arguments.clone();
                value
                    .pointer_mut(&pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert("undeclared".into(), Value::Bool(true));
                (pointer, value)
            })
            .collect()
    }

    pub fn open_values(&self) -> Vec<Value> {
        self.open_objects.iter().map(|pointer| {
            let mut value = self.arguments.clone();
            value.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert("undeclared".into(), serde_json::json!({"arbitrary_provider_value": {"nested": [true, null, {"custom": 1}]}}));
            value
        }).collect()
    }

    pub fn assert_error(&self, location: &str, content: &str) {
        let expected = if location.starts_with("unknown tag ") {
            "unknown_variant"
        } else if location.starts_with("missing ") {
            location.rsplit('/').next().unwrap()
        } else {
            "undeclared"
        };
        assert!(
            content.contains(expected),
            "{} {location}: expected {expected} in {content}",
            self.branch
        );
    }

    pub fn invalid_values(&self) -> Vec<(String, Value)> {
        let mut tag = self.arguments.clone();
        *tag.pointer_mut(&self.tag).expect("fixture tag exists") =
            Value::String("unknown_variant".into());
        let mut values = vec![(format!("unknown tag {}", self.tag), tag)];
        for pointer in &self.required {
            let (parent, field) = pointer.rsplit_once('/').expect("required field pointer");
            let mut value = self.arguments.clone();
            assert!(
                value
                    .pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field)
                    .is_some()
            );
            values.push((format!("missing {pointer}"), value));
        }
        values
    }

    fn objects(&self, value: &Value, pointer: String, objects: &mut Vec<String>) {
        if self.open_objects.contains(&pointer) {
            assert!(value.is_object(), "open map at {pointer}");
            return;
        }
        match value {
            Value::Object(fields) => {
                objects.push(pointer.clone());
                for (name, child) in fields {
                    let token = name.replace('~', "~0").replace('/', "~1");
                    self.objects(child, format!("{pointer}/{token}"), objects);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    self.objects(child, format!("{pointer}/{index}"), objects);
                }
            }
            _ => {}
        }
    }
}
