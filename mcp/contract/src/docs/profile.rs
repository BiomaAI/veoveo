//! Complete immutable owner declarations and their deterministic manual projection.
use super::{CONTRACT_REVISION, ServerDocs, catalog::*};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize};
use veoveo_types::{Check, Checked, ServerSlug};

pub const COMPLIANCE_START: &str = "<!-- veoveo:contract-compliance:start -->";
pub const COMPLIANCE_END: &str = "<!-- veoveo:contract-compliance:end -->";

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ComplianceStatus {
    Met,
    Pending,
    #[vocabulary(rename = "not_applicable")]
    NotApplicable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComplianceItem {
    pub id: RequirementId,
    pub status: ComplianceStatus,
    #[serde(
        default,
        deserialize_with = "provided_note",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "String")]
    pub note: Option<String>,
}

fn provided_note<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, thiserror::Error)]
#[error("invalid compliance profile: {0}")]
pub struct ComplianceError(String);
pub(super) fn invalid(reason: impl Into<String>) -> ComplianceError {
    ComplianceError(reason.into())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileValue {
    server: ServerSlug,
    #[schemars(schema_with = "contract_revision_schema")]
    contract_revision: u32,
    #[schemars(schema_with = "catalog_revision_schema")]
    catalog_revision: u32,
    #[schemars(schema_with = "compliance_schema")]
    compliance: Vec<ComplianceItem>,
}
fn compliance_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema = <Vec<ComplianceItem>>::json_schema(generator);
    schema.insert(
        "minItems".into(),
        serde_json::json!(RequirementId::ALL.len()),
    );
    schema.insert(
        "maxItems".into(),
        serde_json::json!(RequirementId::ALL.len()),
    );
    schema
}
fn contract_revision_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({"type":"integer","const":CONTRACT_REVISION})
}
fn catalog_revision_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({"type":"integer","const":CATALOG_REVISION})
}
impl Check for ProfileValue {
    type Error = ComplianceError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.contract_revision != CONTRACT_REVISION || self.catalog_revision != CATALOG_REVISION
        {
            return Err(invalid(
                "unsupported contract or catalog revision; upgrade the owner profile",
            ));
        }
        if self.compliance.len() != RequirementId::ALL.len() {
            return Err(invalid(
                "every catalog requirement must be declared exactly once",
            ));
        }
        for id in RequirementId::ALL {
            let items: Vec<_> = self
                .compliance
                .iter()
                .filter(|item| item.id == *id)
                .collect();
            if items.len() != 1 {
                return Err(invalid(format!(
                    "{} must be declared exactly once",
                    id.as_str()
                )));
            }
            let item = items[0];
            if item
                .note
                .as_ref()
                .is_some_and(|note| note.trim().is_empty() || note.contains(['\r', '\n']))
            {
                return Err(invalid(format!("{} has an empty explanation", id.as_str())));
            }
            if item.status != ComplianceStatus::Met && item.note.is_none() {
                return Err(invalid(format!("{} requires an explanation", id.as_str())));
            }
            if item.status == ComplianceStatus::NotApplicable
                && id.metadata().applicability.is_none()
            {
                return Err(invalid(format!(
                    "{} has no catalog applicability condition",
                    id.as_str()
                )));
            }
        }
        Ok(())
    }
}

/// Construction and decoding share admission. Accessors never expose mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ComplianceProfile(Checked<ProfileValue>);
impl JsonSchema for ComplianceProfile {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ComplianceProfile".into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::ComplianceProfile").into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        ProfileValue::json_schema(generator)
    }
}

impl ComplianceProfile {
    pub fn new(
        server: ServerSlug,
        compliance: Vec<ComplianceItem>,
    ) -> Result<Self, ComplianceError> {
        Self::admit(ProfileValue {
            server,
            contract_revision: CONTRACT_REVISION,
            catalog_revision: CATALOG_REVISION,
            compliance,
        })
    }
    fn admit(mut value: ProfileValue) -> Result<Self, ComplianceError> {
        value.compliance.sort_by_key(|item| item.id);
        Ok(Self(Checked::new(value)?))
    }
    pub fn server(&self) -> &ServerSlug {
        &self.0.server
    }
    pub fn contract_revision(&self) -> u32 {
        self.0.contract_revision
    }
    pub fn catalog_revision(&self) -> u32 {
        self.0.catalog_revision
    }
    pub fn compliance(&self) -> &[ComplianceItem] {
        &self.0.compliance
    }
    pub fn item(&self, id: RequirementId) -> &ComplianceItem {
        self.compliance()
            .iter()
            .find(|item| item.id == id)
            .expect("admitted complete profile")
    }
    pub fn check_knowledge_applicability(&self, declared: bool) -> Result<(), ComplianceError> {
        let status = self.item(RequirementId::C32).status;
        if declared == (status == ComplianceStatus::NotApplicable) {
            return Err(invalid(
                "C32 applicability disagrees with Discover knowledge-source declaration",
            ));
        }
        Ok(())
    }
}
impl<'de> Deserialize<'de> for ComplianceProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::admit(ProfileValue::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The served declaration uses the admitted author profile without another wire model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ContractDeclaration(ComplianceProfile);
impl ContractDeclaration {
    pub fn from_docs(docs: &ServerDocs) -> Self {
        Self(docs.profile().clone())
    }
    pub fn new(profile: ComplianceProfile) -> Self {
        Self(profile)
    }
    pub fn profile(&self) -> &ComplianceProfile {
        &self.0
    }
    pub fn server(&self) -> &ServerSlug {
        self.0.server()
    }
    pub fn contract_revision(&self) -> u32 {
        self.0.contract_revision()
    }
    pub fn catalog_revision(&self) -> u32 {
        self.0.catalog_revision()
    }
    pub fn compliance(&self) -> &[ComplianceItem] {
        self.0.compliance()
    }
}

pub fn render_compliance(profile: &ComplianceProfile) -> String {
    let mut body = format!(
        "Contract revision: {}\nCatalog revision: {}\n\n",
        profile.contract_revision(),
        profile.catalog_revision()
    );
    for item in profile.compliance() {
        body.push_str(&format!("- {}: {}", item.id.as_str(), item.status.as_str()));
        if let Some(note) = &item.note {
            body.push_str(" — ");
            body.push_str(note);
        }
        body.push('\n');
    }
    format!("{COMPLIANCE_START}\n{body}{COMPLIANCE_END}")
}

/// Strict projection agreement checks the exact served manual bytes, not a parser.
pub fn verify_manual(manual: &str, profile: &ComplianceProfile) -> Result<(), ComplianceError> {
    if manual.matches(COMPLIANCE_START).count() != 1 || manual.matches(COMPLIANCE_END).count() != 1
    {
        return Err(invalid("manual requires one compliance marker pair"));
    }
    let start = manual.find(COMPLIANCE_START).expect("one marker");
    let end_start = manual.find(COMPLIANCE_END).expect("one marker");
    let end = end_start + COMPLIANCE_END.len();
    if start >= end_start || manual.get(start..end) != Some(render_compliance(profile).as_str()) {
        return Err(invalid(
            "manual compliance projection differs from owner profile; run contract-docs",
        ));
    }
    if !manual[..start].ends_with("## Contract Compliance\n\n") {
        return Err(invalid(
            "compliance markers must follow the Contract Compliance heading",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_profile_bytes_enforce_complete_admission() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/compliance-profiles.json")).unwrap();
        for case in fixtures["valid"].as_array().unwrap() {
            let bytes = serde_json::to_vec(&case["profile"]).unwrap();
            let profile: ComplianceProfile = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(profile.compliance().len(), RequirementId::ALL.len());
            assert_eq!(serde_json::to_value(profile).unwrap(), case["profile"]);
        }
        for case in fixtures["invalid"].as_array().unwrap() {
            assert!(
                serde_json::from_value::<ComplianceProfile>(case["profile"].clone()).is_err(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn emitted_profile_schema_admits_shared_valid_bytes_and_closes_structure() {
        let schema = serde_json::to_value(schemars::schema_for!(ComplianceProfile)).unwrap();
        assert_eq!(schema["title"], "ComplianceProfile");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema.get("$ref").is_none());
        let validator = jsonschema::validator_for(&schema).unwrap();
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/compliance-profiles.json")).unwrap();
        for case in fixtures["valid"].as_array().unwrap() {
            assert!(validator.is_valid(&case["profile"]), "{}", case["name"]);
        }
        for invalid in [
            serde_json::json!([]),
            serde_json::json!("profile"),
            serde_json::Value::Null,
        ] {
            assert!(!validator.is_valid(&invalid));
        }
        for field in [
            "server",
            "contractRevision",
            "catalogRevision",
            "compliance",
        ] {
            let mut invalid = fixtures["valid"][0]["profile"].clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(!validator.is_valid(&invalid), "missing {field}");
        }
        for (field, value) in [
            ("contractRevision", CONTRACT_REVISION + 1),
            ("catalogRevision", CATALOG_REVISION + 1),
        ] {
            let mut invalid = fixtures["valid"][0]["profile"].clone();
            invalid[field] = serde_json::json!(value);
            assert!(!validator.is_valid(&invalid), "unsupported {field}");
        }
        let mut invalid = fixtures["valid"][0]["profile"].clone();
        invalid["compliance"][0]["status"] = serde_json::json!("unknown");
        assert!(!validator.is_valid(&invalid));
        invalid = fixtures["valid"][0]["profile"].clone();
        invalid["compliance"].as_array_mut().unwrap().pop();
        assert!(!validator.is_valid(&invalid));
        let mut profile = fixtures["valid"][0]["profile"].clone();
        profile["compliance"][0]["id"] = serde_json::json!("C34");
        assert!(!validator.is_valid(&profile));
        profile = fixtures["valid"][0]["profile"].clone();
        profile["extra"] = serde_json::json!(true);
        assert!(!validator.is_valid(&profile));
        profile = fixtures["valid"][0]["profile"].clone();
        profile["compliance"][0]["note"] = serde_json::Value::Null;
        assert!(!validator.is_valid(&profile));
    }
    #[test]
    fn constructors_and_catalog_growth_require_complete_coverage() {
        let profile: ComplianceProfile =
            serde_json::from_str(include_str!("../../testdata/compliance-example.json")).unwrap();
        for id in RequirementId::ALL {
            assert_eq!(
                serde_json::from_value::<RequirementId>(serde_json::json!(id.as_str())).unwrap(),
                *id
            );
            let items = profile
                .compliance()
                .iter()
                .filter(|item| item.id != *id)
                .cloned()
                .collect();
            assert!(ComplianceProfile::new(profile.server().clone(), items).is_err());
        }
        assert!(serde_json::from_value::<RequirementId>(serde_json::json!("C34")).is_err());
        let mut duplicate = profile.compliance().to_vec();
        duplicate.push(duplicate[0].clone());
        assert!(ComplianceProfile::new(profile.server().clone(), duplicate).is_err());
    }
    #[test]
    fn knowledge_applicability_is_checked_in_both_directions() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/compliance-profiles.json")).unwrap();
        for case in fixtures["valid"].as_array().unwrap() {
            let profile: ComplianceProfile =
                serde_json::from_value(case["profile"].clone()).unwrap();
            let absent = profile.item(RequirementId::C32).status == ComplianceStatus::NotApplicable;
            assert!(profile.check_knowledge_applicability(!absent).is_ok());
            assert!(profile.check_knowledge_applicability(absent).is_err());
        }
    }
    #[test]
    fn renderer_preserves_notes_and_rejects_manual_drift() {
        let profile: ComplianceProfile =
            serde_json::from_str(include_str!("../../testdata/compliance-example.json")).unwrap();
        let manual = format!(
            "# Manual\n\n## Contract Compliance\n\n{}\n",
            render_compliance(&profile)
        );
        assert!(verify_manual(&manual, &profile).is_ok());
        for altered in [
            manual.replace("pending", "met"),
            format!("{manual}\n{COMPLIANCE_START}"),
            manual.replace(COMPLIANCE_END, ""),
        ] {
            assert!(verify_manual(&altered, &profile).is_err());
        }
    }
}
