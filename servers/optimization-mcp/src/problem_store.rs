use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use veoveo_task_runtime::TaskId;

use crate::{
    domain::{
        ConvexProblem, MilpProblem, OptimizationProblemResource, RouteCaseId, RoutingProblem,
    },
    executor::{CompiledMathematicalModel, CompiledRoutingProblem},
};

pub const PREPARED_PROBLEM_VERSION: &str = "veoveo.io/prepared-optimization-problem/v1";
pub const DEFAULT_MAX_PREPARED_PROBLEM_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedRouteCase {
    pub case_id: RouteCaseId,
    pub problem: RoutingProblem,
    pub compiled: CompiledRoutingProblem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
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
pub struct PreparedProblemRef {
    pub path: String,
    pub digest_sha256: String,
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
            digest_sha256,
            bytes: length,
        })
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
        if digest != reference.digest_sha256 {
            anyhow::bail!("prepared problem digest does not match its durable reference");
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn rejects_a_tampered_prepared_problem() {
        let temporary = TempDir::new().unwrap();
        let store = ProblemStore::open(temporary.path(), 1_024).unwrap();
        let reference = PreparedProblemRef {
            path: temporary.path().join("outside.json").display().to_string(),
            digest_sha256: "00".repeat(32),
            bytes: 1,
        };
        tokio::fs::write(&reference.path, b"x").await.unwrap();
        assert!(store.load(&reference).await.is_err());
    }
}
