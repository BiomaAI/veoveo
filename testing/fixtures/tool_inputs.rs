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
    #[serde(default)]
    tag: Option<String>,
    required: Vec<String>,
    #[serde(default)]
    omissions: Vec<String>,
    defaults: BTreeMap<String, Value>,
    open_objects: Vec<String>,
    #[serde(default)]
    typed_dictionaries: BTreeMap<String, Value>,
    #[serde(default)]
    invalid: Vec<InvalidControl>,
    #[serde(default)]
    error_profile: ErrorProfile,
    #[serde(default)]
    object_errors: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ErrorProfile {
    #[default]
    Field,
    Untagged,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidControl {
    pointer: String,
    value: Value,
    error: String,
}

impl ToolInputCase {
    pub fn load(bytes: &[u8]) -> Vec<Self> {
        let cases: Vec<Self> = serde_json::from_slice(bytes).expect("controlled input fixture");
        let branches: BTreeSet<_> = cases.iter().map(|case| case.branch.as_str()).collect();
        assert_eq!(branches.len(), cases.len(), "repeated fixture branch");
        for case in &cases {
            let (_, variant) = case.branch.split_once('.').expect("named branch variant");
            if let Some(tag) = &case.tag {
                assert_eq!(
                    case.arguments.pointer(tag).and_then(Value::as_str),
                    Some(variant),
                    "{} tag does not match its label",
                    case.branch
                );
            } else {
                assert!(
                    !case.invalid.is_empty() || !case.required.is_empty(),
                    "{} untagged branch needs a shape/value negative",
                    case.branch
                );
            }
            for pointer in &case.open_objects {
                assert!(
                    case.arguments
                        .pointer(pointer)
                        .is_some_and(Value::is_object),
                    "{} missing open map at {pointer}",
                    case.branch
                );
            }
            for pointer in case
                .typed_dictionaries
                .keys()
                .chain(case.object_errors.keys())
            {
                assert!(
                    case.arguments
                        .pointer(pointer)
                        .is_some_and(Value::is_object),
                    "{} missing controlled dictionary/object at {pointer}",
                    case.branch
                );
            }
            for pointer in &case.omissions {
                assert!(
                    case.arguments.pointer(pointer).is_some(),
                    "{} missing omission control at {pointer}",
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
        for (pointer, value) in self.omitted_values() {
            let omitted: T = serde_json::from_slice(&serde_json::to_vec(&value).unwrap())
                .unwrap_or_else(|error| panic!("{} omitted {pointer}: {error}", self.branch));
            assert!(
                serde_json::to_value(omitted)
                    .unwrap()
                    .pointer(&pointer)
                    .is_none(),
                "{} omitted {pointer} decoded an unexpected value",
                self.branch
            );
        }
        let serialized = serde_json::to_value(&decoded).unwrap();
        // Untagged labels carry no wire discriminant. Assert the decoded owner value
        // at each explicit shape/value control, rather than treating the label as proof.
        if self.tag.is_none() {
            for pointer in self
                .required
                .iter()
                .chain(self.invalid.iter().map(|control| &control.pointer))
            {
                assert_eq!(
                    serialized.pointer(pointer),
                    self.arguments.pointer(pointer),
                    "{} decoded value at {pointer}",
                    self.branch
                );
            }
        }
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

    /// Admitted optional omissions are positive controls, never hosted mutations.
    pub fn omitted_values(&self) -> Vec<(String, Value)> {
        self.omissions
            .iter()
            .map(|pointer| {
                let (parent, field) = pointer.rsplit_once('/').expect("omission pointer");
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
                (pointer.clone(), value)
            })
            .collect()
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
        let mut values: Vec<_> = self.open_objects.iter().map(|pointer| {
            let mut value = self.arguments.clone();
            value.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert("undeclared".into(), serde_json::json!({"arbitrary_provider_value": {"nested": [true, null, {"custom": 1}]}}));
            value
        }).collect();
        for (pointer, admitted) in &self.typed_dictionaries {
            let mut value = self.arguments.clone();
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("undeclared".into(), admitted.clone());
            values.push(value);
        }
        values
    }

    pub fn assert_error(&self, location: &str, content: &str) {
        if let Some(index) = location.strip_prefix("invalid ") {
            assert!(
                content.contains(&self.invalid[index.parse::<usize>().unwrap()].error),
                "{} {location}: {content}",
                self.branch
            );
            return;
        }
        if let Some(expected) = self.object_errors.get(location) {
            assert!(
                content.contains(expected),
                "{} {location}: {content}",
                self.branch
            );
            return;
        }
        let expected = if location.starts_with("unknown tag ") {
            "unknown_variant"
        } else if location.starts_with("missing ") {
            location.rsplit('/').next().unwrap()
        } else {
            "undeclared"
        };
        assert!(
            content.contains(expected)
                || matches!(self.error_profile, ErrorProfile::Untagged)
                    && content.contains("did not match any variant"),
            "{} {location}: expected {expected} in {content}",
            self.branch
        );
    }

    pub fn invalid_values(&self) -> Vec<(String, Value)> {
        let mut values = Vec::new();
        if let Some(pointer) = &self.tag {
            let mut tag = self.arguments.clone();
            *tag.pointer_mut(pointer).expect("fixture tag exists") =
                Value::String("unknown_variant".into());
            values.push((format!("unknown tag {pointer}"), tag));
        }
        for (index, control) in self.invalid.iter().enumerate() {
            let mut value = self.arguments.clone();
            *value
                .pointer_mut(&control.pointer)
                .expect("fixture invalid control exists") = control.value.clone();
            values.push((format!("invalid {index}"), value));
        }
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
        if self.open_objects.contains(&pointer) || self.typed_dictionaries.contains_key(&pointer) {
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
