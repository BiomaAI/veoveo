//! Source declarations describe prerequisites; they do not authorize effects.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ScenarioFormat {
    #[vocabulary(rename = "veoveo.ai/smoke-scenarios/v1")]
    V1,
}
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use veoveo_types::{Check, Checked};

pub struct ScenarioIdProfile;
impl veoveo_types::IdProfile for ScenarioIdProfile {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.is_empty()
                || value.starts_with('-')
                || value.ends_with('-')
                || value.contains("--")
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(veoveo_types::IdentifierError::new(
                    value,
                    "expected a nonempty kebab-case identity",
                ));
            }
            Ok(())
        });
}
#[veoveo_types::id(text(ScenarioIdProfile))]
pub struct ScenarioId(String);
#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct PackageName(String);
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CargoSelection {
    pub owner: PathBuf,
    pub package: PackageName,
    pub target: String,
    pub features: BTreeSet<String>,
    pub default_features: bool,
    pub profile: BuildProfile,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
pub enum BuildProfile {
    #[vocabulary(rename = "dev")]
    Dev,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum HarnessTarget {
    CargoBinary {
        selection: CargoSelection,
    },
    CargoTest {
        selection: CargoSelection,
        case: String,
        ignored: bool,
    },
    PythonModule {
        module: String,
    },
    Pytest {
        file: PathBuf,
        case: String,
    },
    NodeModule {
        file: PathBuf,
    },
    NodeTest {
        file: PathBuf,
        case: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Preparation {
    CargoBinary {
        selection: CargoSelection,
    },
    CargoTest {
        selection: CargoSelection,
    },
    UvSync {
        owner: PathBuf,
        extras: BTreeSet<String>,
        groups: BTreeSet<String>,
    },
    NpmCi {
        owner: PathBuf,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRequirements {
    pub network: bool,
    pub credentials: bool,
    pub cluster_mutation: bool,
    pub billed_effects: bool,
    pub nvidia: bool,
    pub headed_graphics: bool,
    pub explanation: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionOwnerValue {
    pub owner: PathBuf,
    pub reason: String,
}
impl Check for ExecutionOwnerValue {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        relative(&self.owner)?;
        ensure!(
            !self.reason.trim().is_empty(),
            "composition delegation needs a nonblank reason"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionOwner(Checked<ExecutionOwnerValue>);
impl std::ops::Deref for ExecutionOwner {
    type Target = ExecutionOwnerValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scenario {
    pub id: ScenarioId,
    pub description: String,
    pub target: HarnessTarget,
    /// Explicit composition delegation; omission selects the containing owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_owner: Option<ExecutionOwner>,
    pub arguments: Vec<String>,
    pub prerequisites: Vec<Preparation>,
    pub requirements: RuntimeRequirements,
    pub deadline_seconds: u64,
    pub cleanup_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioDescriptorValue {
    pub format: ScenarioFormat,
    pub owner: PathBuf,
    pub scenarios: Vec<Scenario>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScenarioDescriptor(Checked<ScenarioDescriptorValue>);
impl std::ops::Deref for ScenarioDescriptor {
    type Target = ScenarioDescriptorValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl ScenarioDescriptorValue {
    pub fn build(self) -> Result<ScenarioDescriptor> {
        Ok(ScenarioDescriptor(Checked::new(self)?))
    }
}
impl Check for ScenarioDescriptorValue {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        ensure!(
            self.format == ScenarioFormat::V1,
            "unsupported smoke declaration revision"
        );
        relative(&self.owner)?;
        ensure!(!self.scenarios.is_empty(), "owner has no scenarios");
        let mut ids = BTreeSet::new();
        for scenario in &self.scenarios {
            ensure!(
                ids.insert(&scenario.id),
                "duplicate smoke scenario identity"
            );
            ensure!(
                !scenario.description.trim().is_empty(),
                "scenario description is blank"
            );
            ensure!(
                scenario.deadline_seconds > 0
                    && scenario.cleanup_seconds > 0
                    && scenario
                        .deadline_seconds
                        .checked_add(scenario.cleanup_seconds)
                        .is_some_and(|total| std::time::Instant::now()
                            .checked_add(std::time::Duration::from_secs(total))
                            .is_some()),
                "invalid execution/cleanup budgets"
            );
            ensure!(
                !scenario.requirements.explanation.trim().is_empty(),
                "runtime prerequisites need an explanation"
            );
            ensure!(
                scenario.arguments.iter().all(|value| !value.contains('\0')),
                "invalid native argument"
            );
            if let Some(owner) = &scenario.execution_owner {
                owner.0.get().check()?;
            }
            match &scenario.target {
                HarnessTarget::CargoBinary { selection }
                | HarnessTarget::CargoTest { selection, .. } => selection.check()?,
                HarnessTarget::PythonModule { module } => ensure!(
                    !module.is_empty()
                        && module.split('.').all(|part| !part.is_empty()
                            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')),
                    "invalid Python module"
                ),
                HarnessTarget::Pytest { file, case } | HarnessTarget::NodeTest { file, case } => {
                    relative(file)?;
                    ensure!(
                        !case.is_empty() && !case.contains('\0'),
                        "exact framework case is required"
                    );
                }
                HarnessTarget::NodeModule { file } => relative(file)?,
            }
            if let HarnessTarget::CargoTest { case, .. } = &scenario.target {
                ensure!(
                    !case.is_empty() && !case.contains('\0'),
                    "exact Rust case is required"
                );
            }
            let mut native_roots = std::collections::BTreeMap::new();
            let mut admit_root = |selection: &CargoSelection, test: bool| -> Result<()> {
                let key = (selection.package.clone(), selection.target.clone(), test);
                if let Some(previous) = native_roots.insert(key, selection.clone()) {
                    ensure!(
                        previous == *selection,
                        "conflicting Cargo feature/profile selections for one native target"
                    );
                }
                Ok(())
            };
            match &scenario.target {
                HarnessTarget::CargoBinary { selection } => admit_root(selection, false)?,
                HarnessTarget::CargoTest { selection, .. } => admit_root(selection, true)?,
                _ => {}
            }
            let mut prerequisites = BTreeSet::new();
            for prerequisite in &scenario.prerequisites {
                ensure!(
                    prerequisites.insert(serde_json::to_string(prerequisite)?),
                    "duplicate build prerequisite"
                );
                match prerequisite {
                    Preparation::CargoBinary { selection } => {
                        selection.check()?;
                        admit_root(selection, false)?;
                    }
                    Preparation::CargoTest { selection } => {
                        selection.check()?;
                        admit_root(selection, true)?;
                    }
                    Preparation::UvSync { owner, .. } | Preparation::NpmCi { owner } => {
                        relative(owner)?
                    }
                }
            }
        }
        Ok(())
    }
}
impl CargoSelection {
    pub fn check(&self) -> Result<()> {
        relative(&self.owner)?;
        ensure!(
            !self.target.is_empty() && !self.target.chars().any(char::is_whitespace),
            "invalid native target identity"
        );
        ensure!(
            self.features.iter().all(|f| !f.is_empty()
                && f.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-/".contains(&b))),
            "invalid Cargo feature"
        );
        Ok(())
    }
}
pub fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "source path must be normalized and owner-relative"
    );
    Ok(())
}
pub fn contained(root: &Path, relative_path: &Path) -> Result<PathBuf> {
    relative(relative_path)?;
    let root = root.canonicalize()?;
    let path = root.join(relative_path).canonicalize()?;
    ensure!(
        path.starts_with(&root),
        "source path escapes its admitted owner"
    );
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn independent_owner() -> ScenarioDescriptorValue {
        serde_json::from_str(include_str!(
            "../../fixtures/modular-mcp/smoke/scenarios.json"
        ))
        .unwrap()
    }
    #[test]
    fn independent_descriptor_has_no_core_scenario_registration() {
        let admitted = independent_owner().build().unwrap();
        assert_eq!(admitted.owner, Path::new("testing/fixtures/modular-mcp"));
        assert_eq!(admitted.scenarios.len(), 1);
        let HarnessTarget::CargoTest {
            selection,
            case,
            ignored,
        } = &admitted.scenarios[0].target
        else {
            panic!("expected actual owner framework target")
        };
        assert_eq!(selection.package.as_str(), "veoveo-modular-fixture-mcp");
        assert_eq!(selection.features, BTreeSet::from(["contract".to_owned()]));
        assert_eq!(
            case,
            "independent_owned_addresses_and_scope_names_round_trip"
        );
        assert!(!ignored);
    }
    #[test]
    fn admission_rejects_duplicate_identity_missing_case_and_unbounded_budgets() {
        let mut value = independent_owner();
        value.scenarios.push(value.scenarios[0].clone());
        assert!(value.build().is_err());
        for (deadline, cleanup) in [(0, 1), (1, 0), (u64::MAX, 1), (u64::MAX - 1, 1)] {
            let mut value = independent_owner();
            value.scenarios[0].deadline_seconds = deadline;
            value.scenarios[0].cleanup_seconds = cleanup;
            assert!(value.build().is_err());
        }
        let mut value = independent_owner();
        if let HarnessTarget::CargoTest { case, .. } = &mut value.scenarios[0].target {
            case.clear();
        }
        assert!(value.build().is_err());
        let mut bytes: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/modular-mcp/smoke/scenarios.json"
        ))
        .unwrap();
        bytes["scenarios"][0]["target"]["selection"]["typo"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ScenarioDescriptor>(bytes).is_err());
    }
    #[test]
    fn initial_descriptor_rejects_old_keys_unknown_versions_and_unexplained_delegation() {
        let value = serde_json::to_value(independent_owner().build().unwrap()).unwrap();
        assert!(value["scenarios"][0].get("deadlineSeconds").is_some());
        for (old, new) in [
            ("deadline_seconds", "deadlineSeconds"),
            ("cleanup_seconds", "cleanupSeconds"),
        ] {
            let mut bad = value.clone();
            let row = bad["scenarios"][0].as_object_mut().unwrap();
            let v = row.remove(new).unwrap();
            row.insert(old.into(), v);
            assert!(serde_json::from_value::<ScenarioDescriptor>(bad).is_err());
        }
        let mut old_selection = value.clone();
        let selection = old_selection["scenarios"][0]["target"]["selection"]
            .as_object_mut()
            .unwrap();
        let default = selection.remove("defaultFeatures").unwrap();
        selection.insert("default_features".into(), default);
        assert!(serde_json::from_value::<ScenarioDescriptor>(old_selection).is_err());
        for (old, new) in [
            ("cluster_mutation", "clusterMutation"),
            ("billed_effects", "billedEffects"),
            ("headed_graphics", "headedGraphics"),
        ] {
            let mut old_requirements = value.clone();
            let requirements = old_requirements["scenarios"][0]["requirements"]
                .as_object_mut()
                .unwrap();
            let entry = requirements.remove(new).unwrap();
            requirements.insert(old.into(), entry);
            assert!(serde_json::from_value::<ScenarioDescriptor>(old_requirements).is_err());
        }
        let mut bad = value.clone();
        bad["format"] = serde_json::json!("veoveo.ai/smoke-scenarios/v2");
        assert!(serde_json::from_value::<ScenarioDescriptor>(bad).is_err());
        let mut bad = value;
        bad["scenarios"][0]["executionOwner"] =
            serde_json::json!({"owner":"../foreign","reason":" "});
        assert!(serde_json::from_value::<ScenarioDescriptor>(bad).is_err());
    }
    #[test]
    fn source_containment_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let owner = tempfile::tempdir().unwrap();
        let foreign = tempfile::tempdir().unwrap();
        std::fs::write(foreign.path().join("case.rs"), "owned elsewhere").unwrap();
        symlink(foreign.path(), owner.path().join("linked")).unwrap();
        assert!(contained(owner.path(), Path::new("linked/case.rs")).is_err());
    }
    #[test]
    fn rejects_unsafe_source_and_scenario_identities() {
        for p in ["../other", "/absolute", "a/../b", "./a"] {
            assert!(relative(Path::new(p)).is_err());
        }
        for id in ["", "Mixed", "a_b", "a--b", "a/child"] {
            assert!(ScenarioId::parse(id).is_err());
        }
        assert!(ScenarioId::parse("independent-reading-check").is_ok());
    }
}
