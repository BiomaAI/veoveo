//! Optional in-memory owner evidence; no resource-body discovery protocol is invented.
use anyhow::{Result, ensure};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::Value;
use veoveo_types::NamingLabel;

/// Where inspection obtained a schema or an observed value. Source assertions are
/// never reported as remote observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum SchemaEvidenceOrigin {
    Remote,
    OwnerObserved,
    SourceOnly,
}

/// Deliberately supplied owner data. The owner must omit secrets and unsafe data;
/// reports contain counts and locations, never these values.
pub struct SchemaObservation(Value);
impl SchemaObservation {
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self> {
        Ok(Self(serde_json::to_value(value)?))
    }
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}

/// A generated schema from the actual owner type and optional safe observations.
/// Absence of observations records source-only inspection, not a passed body probe.
pub struct OwnerSchemaEvidence {
    label: NamingLabel,
    schema: Schema,
    definitions_path: String,
    observations: Vec<SchemaObservation>,
}
impl OwnerSchemaEvidence {
    pub fn generated<T: JsonSchema>(label: NamingLabel) -> Self {
        Self::generated_with::<T>(label, SchemaGenerator::default())
    }
    pub fn generated_with<T: JsonSchema>(label: NamingLabel, generator: SchemaGenerator) -> Self {
        let definitions_path = generator.settings().definitions_path.to_string();
        Self {
            label,
            schema: generator.into_root_schema_for::<T>(),
            definitions_path,
            observations: vec![],
        }
    }
    pub fn observe(mut self, observation: SchemaObservation) -> Self {
        self.observations.push(observation);
        self
    }
    pub fn origin(&self) -> SchemaEvidenceOrigin {
        if self.observations.is_empty() {
            SchemaEvidenceOrigin::SourceOnly
        } else {
            SchemaEvidenceOrigin::OwnerObserved
        }
    }
    pub(crate) fn schema(&self) -> &Schema {
        &self.schema
    }
    pub(crate) fn definitions_path(&self) -> &str {
        &self.definitions_path
    }
    pub(crate) fn label(&self) -> &str {
        self.label.as_str()
    }
    pub(crate) fn observations(&self) -> &[SchemaObservation] {
        &self.observations
    }
}

/// Explicit endpoint projection. No production server-name registry or inference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ToolNameProjection {
    #[default]
    Local,
    Gateway,
}

/// Owner-supplied body selections travel in memory, outside the public MCP wire.
/// Required labels make omission visible rather than treating missing data as a skip.
#[derive(Default)]
pub struct NamingEvidence<'a> {
    pub tool_names: ToolNameProjection,
    pub required_observations: Vec<NamingLabel>,
    pub bodies: &'a [OwnerSchemaEvidence],
}
impl NamingEvidence<'_> {
    pub(crate) fn has_required_observations(&self) -> bool {
        if self.bodies.len() > crate::naming::MAX_NAMING_ROOTS
            || self.required_observations.len() > crate::naming::MAX_NAMING_ROOTS
        {
            return false;
        }
        let observed: std::collections::BTreeSet<_> = self
            .bodies
            .iter()
            .filter(|body| !body.observations().is_empty())
            .map(|body| body.label())
            .collect();
        self.required_observations
            .iter()
            .all(|required| observed.contains(required.as_str()))
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let mut required = std::collections::BTreeSet::new();
        for label in &self.required_observations {
            ensure!(
                required.insert(label.as_str()),
                "duplicate required observation label"
            );
        }
        let mut labels = std::collections::BTreeSet::new();
        for body in self.bodies {
            ensure!(
                labels.insert(body.label()),
                "duplicate owner evidence label"
            );
        }
        let observed: std::collections::BTreeSet<_> = self
            .bodies
            .iter()
            .filter(|body| !body.observations().is_empty())
            .map(|body| body.label())
            .collect();
        for required in &self.required_observations {
            ensure!(
                observed.contains(required.as_str()),
                "required owner observations are missing"
            );
        }
        Ok(())
    }
}
