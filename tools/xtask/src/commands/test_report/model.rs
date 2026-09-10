//! Closed receipt and index shapes. Source validity never renews runtime evidence.
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(super) const RECEIPT_SCHEMA: &str = "veoveo.io/test-receipt/v1";
pub(super) const INDEX_SCHEMA: &str = "veoveo.io/local-test-report/v3";
pub(super) const PROFILE_SCHEMA: &str = "veoveo.io/test-coverage-profile/v1";
pub(super) const PLANNER_VERSION: u32 = 1;
pub(super) const INDEX_PATH: &str = "testing/local-test-report.json";
pub(super) const RECEIPT_DIRECTORY: &str = "testing/test-receipts";
pub(super) const CATALOG_PATH: &str = "testing/evidence-checks.json";
pub(super) const CATALOG_SCHEMA: &str = "veoveo.io/test-check-catalog/v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileInput {
    pub path: String,
    pub content: FileContent,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum FileContent {
    File {
        sha256: String,
        bytes: u64,
        mode: u32,
    },
    Symlink {
        target: String,
    },
    Missing,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum InputScope {
    Repository,
    Cargo {
        packages: Vec<String>,
        roots: Vec<String>,
    },
    Console {
        roots: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceInputs {
    pub planner_version: u32,
    pub scope: InputScope,
    pub digest: String,
    pub files: Vec<FileInput>,
}

/// Only admitted command syntax is persisted. An arbitrary invocation may contain
/// secrets in positional arguments, flags or environment assignments.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum CommandIdentity {
    Admitted { arguments: Vec<String> },
    Opaque { program: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum EvidenceClass {
    Source,
    Integration,
    Installed,
    Visual,
    Unclassified,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ToolIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Toolchain {
    Rust,
    Node,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CheckDefinition {
    pub arguments: Vec<String>,
    pub inputs: InputScope,
    pub toolchains: Vec<Toolchain>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CheckCatalog {
    pub schema_version: String,
    pub checks: Vec<CheckDefinition>,
}

/// Values are public identities such as an OCI digest, configuration digest or
/// installation-owned secret revision. Secret material itself is never admitted.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeIdentity {
    pub class: EvidenceClass,
    pub bindings: BTreeMap<String, String>,
    pub valid_for_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EnvironmentIdentity {
    pub os: String,
    pub architecture: String,
    pub toolchains: Vec<ToolIdentity>,
    pub runtime: RuntimeIdentity,
    pub reusable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceProvenance {
    pub revision: Option<String>,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Outcome {
    Passed,
    Failed,
    InputsChanged,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema_version: String,
    pub run_id: Uuid,
    pub name: String,
    pub check_id: String,
    pub command: CommandIdentity,
    pub provenance: SourceProvenance,
    pub inputs: SourceInputs,
    pub environment: EnvironmentIdentity,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_millis: u64,
    pub outcome: Outcome,
    pub exit_code: Option<i32>,
    pub diagnostics: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReceiptRef {
    pub run_id: Uuid,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReceiptIndex {
    pub schema_version: String,
    pub updated_at: DateTime<Utc>,
    pub receipts: Vec<ReceiptRef>,
    pub latest: BTreeMap<String, Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CoverageRequirement {
    pub check_id: String,
    pub class: EvidenceClass,
    pub max_age_seconds: Option<u64>,
    pub bindings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CoverageProfile {
    pub schema_version: String,
    pub name: String,
    pub requirements: Vec<CoverageRequirement>,
}
