use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use veoveo_types::TaskId;

use crate::{
    contract::{
        ConvexProblem, MilpProblem, OptimizationProblemResource, RouteCaseId, RoutingProblem,
    },
    executor::{CompiledMathematicalModel, CompiledRoutingProblem},
};

pub const DEFAULT_MAX_PREPARED_PROBLEM_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedRouteCase {
    pub case_id: RouteCaseId,
    pub problem: RoutingProblem,
    pub compiled: CompiledRoutingProblem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "family",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PreparedProblem {
    Routing {
        resource: OptimizationProblemResource,
        problem: RoutingProblem,
        compiled: CompiledRoutingProblem,
    },
    RouteScenarios {
        resource: OptimizationProblemResource,
        cases: Vec<PreparedRouteCase>,
    },
    Convex {
        resource: OptimizationProblemResource,
        problem: ConvexProblem,
        compiled: CompiledMathematicalModel,
    },
    Milp {
        resource: OptimizationProblemResource,
        problem: MilpProblem,
        compiled: CompiledMathematicalModel,
    },
}

impl PreparedProblem {
    pub fn resource(&self) -> &OptimizationProblemResource {
        match self {
            Self::Routing { resource, .. }
            | Self::RouteScenarios { resource, .. }
            | Self::Convex { resource, .. }
            | Self::Milp { resource, .. } => resource,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedProblemRef {
    pub path: String,
    pub digest_sha256: veoveo_artifact_contract::UploadSha256,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ProblemStore {
    root: Arc<PathBuf>,
    maximum_bytes: u64,
}

impl ProblemStore {
    pub fn open(root: impl Into<PathBuf>, maximum_bytes: u64) -> anyhow::Result<Self> {
        if maximum_bytes == 0 {
            anyhow::bail!("maximum prepared-problem bytes must be positive");
        }
        let root = root.into();
        if !root.is_absolute() {
            anyhow::bail!("optimization workspace must be absolute");
        }
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        Ok(Self {
            root: Arc::new(root),
            maximum_bytes,
        })
    }

    pub async fn stage(
        &self,
        task_id: TaskId,
        problem: &PreparedProblem,
    ) -> anyhow::Result<PreparedProblemRef> {
        let bytes = serde_json::to_vec(problem)?;
        let length = bytes.len() as u64;
        if length > self.maximum_bytes {
            anyhow::bail!(
                "prepared problem is {length} bytes and exceeds the {}-byte limit",
                self.maximum_bytes
            );
        }
        let digest_sha256 = hex::encode(Sha256::digest(&bytes));
        let task_dir = self.root.join(task_id.to_string());
        tokio::fs::create_dir_all(&task_dir).await?;
        let final_path = task_dir.join("prepared-problem.json");
        let temporary_path = task_dir.join("prepared-problem.pending");
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary_path, &final_path).await?;
        Ok(PreparedProblemRef {
            path: final_path.to_string_lossy().into_owned(),
            digest_sha256: veoveo_artifact_contract::UploadSha256::parse(digest_sha256)?,
            bytes: length,
        })
    }

    /// Bind an admitted stored product to the Task selected by SQL policy.
    pub async fn load_selected(
        &self,
        reference: &PreparedProblemRef,
        problem_id: &crate::contract::ProblemId,
        family: crate::contract::ProblemFamily,
    ) -> anyhow::Result<PreparedProblem> {
        let prepared = self.load(reference).await?;
        admit_prepared_product(&prepared, problem_id, family)?;
        Ok(prepared)
    }

    pub async fn load(&self, reference: &PreparedProblemRef) -> anyhow::Result<PreparedProblem> {
        if reference.bytes > self.maximum_bytes {
            anyhow::bail!("persisted prepared problem exceeds the configured byte limit");
        }
        let path = Path::new(&reference.path);
        let canonical = tokio::fs::canonicalize(path).await?;
        if canonical == *self.root || !canonical.starts_with(self.root.as_path()) {
            anyhow::bail!("prepared problem path escapes the optimization workspace");
        }
        let metadata = tokio::fs::metadata(&canonical).await?;
        if !metadata.is_file() || metadata.len() != reference.bytes {
            anyhow::bail!("prepared problem size does not match its durable reference");
        }
        let bytes = tokio::fs::read(&canonical).await?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != reference.digest_sha256.as_str() {
            anyhow::bail!("prepared problem digest does not match its durable reference");
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// Check selected context and the producer's duplicate resource definition.
pub fn admit_prepared_product(
    prepared: &PreparedProblem,
    problem_id: &crate::contract::ProblemId,
    family: crate::contract::ProblemFamily,
) -> anyhow::Result<()> {
    let resource = prepared.resource();
    anyhow::ensure!(
        &resource.record.problem_id == problem_id && resource.record.family == family,
        "prepared product disagrees with its selected Task parents"
    );
    use crate::contract::{OptimizationProblemDefinition as Definition, RoutingProblemSource};
    let agrees = match (prepared, &resource.definition) {
            (PreparedProblem::Routing { problem, .. }, Definition::Routing { problem: original }) => problem == original,
            (PreparedProblem::Convex { problem, .. }, Definition::Convex { problem: original }) => problem == original,
            (PreparedProblem::Milp { problem, .. }, Definition::Milp { problem: original }) => problem == original,
            (PreparedProblem::RouteScenarios { cases, .. }, Definition::RouteScenarios { cases: original }) => cases.len() == original.len() && cases.iter().zip(original).all(|(case, original)| case.case_id == original.case_id && matches!(&original.problem, RoutingProblemSource::Inline { problem } if problem == &case.problem)),
            _ => false,
        };
    anyhow::ensure!(
        agrees,
        "prepared product definition disagrees with its selected resource"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn actual_prepared_load_binds_selected_parents_and_duplicate_definition() {
        use crate::contract::*;
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/controlled-inputs.json")).unwrap();
        let problem: ConvexProblem =
            serde_json::from_value(fixtures[0]["arguments"]["problem"]["problem"].clone()).unwrap();
        let compiled = crate::compiler::compile_convex_problem(&problem).unwrap();
        let definition = OptimizationProblemDefinition::Convex {
            problem: problem.clone(),
        };
        let id = ProblemId::new();
        let now = chrono::Utc::now();
        let resource = OptimizationProblemResourceValue {
            record: OptimizationProblemRecordValue {
                problem_id: id.clone(),
                problem_uri: OptimizationProblemUri::new(id.clone()).unwrap(),
                family: ProblemFamily::Convex,
                schema_version: problem.version.clone(),
                digest_sha256: definition_digest(&definition).unwrap(),
                dimensions: ProblemDimensions {
                    variables: Some(1),
                    constraints: Some(0),
                    nonzeros: Some(0),
                    ..Default::default()
                },
                authority: OptimizationAuthority {
                    principal_id: "fixture#actor".parse().unwrap(),
                    work_context: None,
                    policy_revision: "fixture".parse().unwrap(),
                    submitted_at: now,
                },
                created_at: now,
            }
            .build()
            .unwrap(),
            definition,
        }
        .build()
        .unwrap();
        let prepared = PreparedProblem::Convex {
            resource,
            problem: problem.clone(),
            compiled,
        };
        let temporary = TempDir::new().unwrap();
        let store = ProblemStore::open(temporary.path(), 1024 * 1024).unwrap();
        let reference = store.stage(TaskId::new(), &prepared).await.unwrap();
        let before = tokio::fs::read(&reference.path).await.unwrap();
        assert!(
            store
                .load_selected(&reference, &id, ProblemFamily::Convex)
                .await
                .is_ok()
        );
        assert!(
            store
                .load_selected(&reference, &ProblemId::new(), ProblemFamily::Convex)
                .await
                .is_err()
        );
        assert!(
            store
                .load_selected(&reference, &id, ProblemFamily::Milp)
                .await
                .is_err()
        );
        let mut foreign = prepared.clone();
        if let PreparedProblem::Convex { resource, .. } = &mut foreign {
            let mut value = OptimizationProblemResourceValue::from(resource.clone());
            let mut record = OptimizationProblemRecordValue::from(value.record);
            record.problem_id = ProblemId::new();
            record.problem_uri = OptimizationProblemUri::new(record.problem_id.clone()).unwrap();
            value.record = record.build().unwrap();
            *resource = value.build().unwrap();
        }
        let foreign_reference = store.stage(TaskId::new(), &foreign).await.unwrap();
        assert!(store.load(&foreign_reference).await.is_ok());
        assert!(
            store
                .load_selected(&foreign_reference, &id, ProblemFamily::Convex)
                .await
                .is_err()
        );
        let mut detached = prepared.clone();
        if let PreparedProblem::Convex { problem, .. } = &mut detached {
            let mut draft = ConvexProblemValue::from(problem.clone());
            draft.objective.linear_terms[0].coefficient = FiniteF64::new(2.0).unwrap();
            *problem = draft.build().unwrap();
        }
        let corrupted = store.stage(TaskId::new(), &detached).await.unwrap();
        assert!(store.load(&corrupted).await.is_ok());
        assert!(
            store
                .load_selected(&corrupted, &id, ProblemFamily::Convex)
                .await
                .is_err()
        );
        let current: serde_json::Value = serde_json::from_slice(&before).unwrap();
        for (path, key, retired) in [
            ("/resource/record", "problemId", "problem_id"),
            ("/resource/record", "schemaVersion", "schema_version"),
            ("/resource/record", "digestSha256", "digest_sha256"),
            ("/compiled", "variableIds", "variable_ids"),
        ] {
            for mixed in [false, true] {
                let mut invalid = current.clone();
                let object = invalid.pointer_mut(path).unwrap().as_object_mut().unwrap();
                let value = if mixed {
                    object[key].clone()
                } else {
                    object.remove(key).unwrap()
                };
                object.insert(retired.into(), value);
                let bytes = serde_json::to_vec(&invalid).unwrap();
                tokio::fs::write(&reference.path, &bytes).await.unwrap();
                let mut current_reference = reference.clone();
                current_reference.bytes = bytes.len() as u64;
                current_reference.digest_sha256 = veoveo_artifact_contract::UploadSha256::parse(
                    hex::encode(Sha256::digest(&bytes)),
                )
                .unwrap();
                assert!(
                    store.load(&current_reference).await.is_err(),
                    "{path}/{key}, mixed={mixed}"
                );
                assert_eq!(
                    tokio::fs::read(&reference.path).await.unwrap(),
                    bytes,
                    "receiver must not repair input"
                );
            }
        }
        for mixed in [false, true] {
            let mut wire = serde_json::to_value(&reference).unwrap();
            let object = wire.as_object_mut().unwrap();
            let value = if mixed {
                object["digestSha256"].clone()
            } else {
                object.remove("digestSha256").unwrap()
            };
            object.insert("digest_sha256".into(), value);
            assert!(serde_json::from_value::<PreparedProblemRef>(wire).is_err());
        }
        tokio::fs::write(&reference.path, &before).await.unwrap();
        assert_eq!(tokio::fs::read(&reference.path).await.unwrap(), before);
    }

    #[tokio::test]
    async fn rejects_a_tampered_prepared_problem() {
        let temporary = TempDir::new().unwrap();
        let store = ProblemStore::open(temporary.path(), 1_024).unwrap();
        let reference = PreparedProblemRef {
            path: temporary.path().join("outside.json").display().to_string(),
            digest_sha256: veoveo_artifact_contract::UploadSha256::parse("00".repeat(32)).unwrap(),
            bytes: 1,
        };
        tokio::fs::write(&reference.path, b"x").await.unwrap();
        assert!(store.load(&reference).await.is_err());
    }
}
