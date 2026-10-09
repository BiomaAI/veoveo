//! Private, independently supplied completed corpus. Admission performs no I/O to services.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use veoveo_mcp_contract::UsageReport;
use veoveo_optimization_mcp::contract::*;
use veoveo_testing_support::installed::knowledge as installed;
use veoveo_types::{CanonicalTaskId, HttpsUrl, TaskId, WorkContextId};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub installation: installed::InstalledSource,
    pub corpus_file: PathBuf,
    pub alternate_token_file: PathBuf,
    pub alternate_context: WorkContextId,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Corpus {
    pub schema: String,
    pub visible: Vec<Solve>,
    pub denied: Vec<Solve>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Solve {
    pub task_id: TaskId,
    pub gateway_task_id: CanonicalTaskId,
    pub problem: OptimizationProblemResource,
    pub run: OptimizationRunRecord,
    pub solution: OptimizationSolution,
    pub output: OptimizationToolOutput,
    pub usage: UsageReport,
}

pub fn private_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    ensure!(path.is_absolute(), "private fixture path must be absolute");
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "private fixture must be a bounded regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "fixture file must be private"
        );
    }
    let file = fs::File::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        ensure!(
            metadata.dev() == opened.dev() && metadata.ino() == opened.ino(),
            "fixture changed during admission"
        );
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "fixture exceeded its byte budget"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

impl Input {
    pub fn load() -> Result<Self> {
        let path = std::env::var_os("VEOVEO_OPTIMIZATION_CONSUMERS_INPUT")
            .ok_or_else(|| anyhow::anyhow!("set VEOVEO_OPTIMIZATION_CONSUMERS_INPUT"))?;
        private_json(Path::new(&path), 64 * 1024)
    }
    pub fn admit(&mut self) -> Result<Corpus> {
        let target = self.installation.validate()?;
        ensure!(
            self.installation.deployment == "optimization-mcp"
                && target
                    .expected_deployments
                    .iter()
                    .any(|d| d == "optimization-mcp")
                && target.minimum_gpu_shares > 0,
            "fixture requires declared GPU Optimization installation"
        );
        // Installation identity owns the route; supplied endpoint must agree exactly.
        let mut endpoint = target.public_base_url.clone();
        endpoint
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid public origin"))?
            .clear()
            .extend(["mcp", target.operator.profile.as_str()]);
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        let endpoint = HttpsUrl::parse(endpoint.as_str())?;
        ensure!(
            self.installation.endpoint == endpoint,
            "caller endpoint must match installed operator profile"
        );
        self.installation.endpoint = endpoint;
        ensure!(
            self.alternate_token_file != self.installation.caller_token_file,
            "alternate caller requires separate private token"
        );
        installed::credentials(&self.installation.caller_token_file)?;
        installed::credentials(&self.alternate_token_file)?;
        let corpus: Corpus = private_json(&self.corpus_file, 8 * 1024 * 1024)?;
        corpus.admit()?;
        let context = WorkContextId::parse(&target.operator.work_context)?;
        ensure!(
            corpus.visible[0]
                .problem
                .record
                .authority
                .work_context
                .as_ref()
                == Some(&context),
            "visible corpus must bind installation operator Work Context"
        );
        let comparison =
            target.operator.comparison_context.as_ref().ok_or_else(|| {
                anyhow::anyhow!("declare comparisonContext for the alternate caller")
            })?;
        ensure!(
            self.alternate_context == WorkContextId::parse(comparison)?
                && self.alternate_context != context,
            "alternate caller context must match the distinct installed comparisonContext"
        );
        Ok(corpus)
    }
}
impl Solve {
    pub fn admit(&self) -> Result<()> {
        ensure!(
            self.task_id.as_uuid().get_version_num() == 7,
            "expected native UUIDv7 Task identity"
        );
        let p = &self.problem.record;
        let s = &self.solution;
        let r = &self.run;
        let o = &self.output;
        ensure!(
            r.phase == RunPhase::Completed
                && r.problem_uri == p.problem_uri
                && r.run_id == s.run_id
                && r.run_uri == o.run_uri
                && r.solution_uri.as_ref() == Some(&s.solution_uri)
                && p.problem_uri == s.problem_uri
                && p.problem_uri == o.problem_uri
                && s.solution_uri == o.result_uri
                && r.family == p.family
                && p.family == o.family
                && s.feasibility == o.feasibility
                && s.termination == o.termination,
            "fixture parents or completed outcome disagree"
        );
        ensure!(
            p.authority == s.authority && r.authority == s.authority && r.engine == s.engine,
            "fixture authority or engine provenance disagree"
        );
        ensure!(
            s.engine.name == "NVIDIA cuOpt"
                && s.engine.version == CUOPT_STABLE_VERSION
                && s.engine.container_digest == CUOPT_CONTAINER_DIGEST
                && s.engine.executor_protocol == EXECUTOR_PROTOCOL_VERSION
                && [
                    &s.engine.gpu_uuid,
                    &s.engine.gpu_name,
                    &s.engine.compute_capability
                ]
                .iter()
                .all(|value| value
                    .as_ref()
                    .is_some_and(|v| !v.is_empty() && v.len() <= 256)),
            "fixture requires retained pinned real-GPU producer provenance"
        );
        ensure!(
            self.usage.task_id == self.task_id.to_string()
                && self.usage.usage_uri == OptimizationTaskUsageUri::new(self.task_id)?.as_str()
                && !self.usage.records.is_empty()
                && self.usage.records.len() <= 16
                && self
                    .usage
                    .records
                    .iter()
                    .all(|record| record.task_id == self.usage.task_id),
            "fixture usage must bind its native Task"
        );
        // Reuse the owner's selected-parent/family and digest admission, rather than
        // treating a valid digest as proof that this is the selected solve's product.
        veoveo_optimization_mcp::solution_builder::admit_solution_bytes(
            &serde_json::to_vec(s)?,
            &o.result_uri,
            &r.run_id,
            &p.problem_uri,
            p.family,
        )?;
        // Exact observations compare independently retained feasibility and findings.
        // These consumer expectations do not independently rerun the solver.
        ensure!(
            o.problem_artifact.byte_len == serde_json::to_vec(&self.problem)?.len() as u64
                && o.solution_artifact.byte_len == serde_json::to_vec(&self.solution)?.len() as u64,
            "fixture canonical Artifact lengths disagree with selected products"
        );
        for metadata in [&o.problem_artifact, &o.solution_artifact] {
            ensure!(
                metadata.compliance.work_context == p.authority.work_context
                    && metadata
                        .compliance
                        .provenance
                        .as_ref()
                        .is_some_and(|provenance| provenance.producer == p.authority.principal_id
                            && provenance.policy_revision == p.authority.policy_revision),
                "canonical Artifact context or producer provenance differs"
            );
            ensure!(
                metadata.byte_len > 0 && metadata.byte_len <= 256 * 1024,
                "canonical fixture artifact exceeds read budget"
            );
        }
        Ok(())
    }
}
impl Corpus {
    pub fn admit(&self) -> Result<()> {
        ensure!(
            self.schema == "veoveo.ai/optimization-consumer-fixture/v1",
            "unsupported fixture version"
        );
        ensure!(
            (101..=128).contains(&self.visible.len()) && (1..=8).contains(&self.denied.len()),
            "fixture requires 101..128 visible completed solves and 1..8 denied solves"
        );
        let mut tasks = BTreeSet::new();
        let mut gateway_tasks = BTreeSet::new();
        let mut problems = BTreeSet::new();
        let mut runs = BTreeSet::new();
        let mut solutions = BTreeSet::new();
        for solve in self.visible.iter().chain(&self.denied) {
            solve.admit()?;
            ensure!(
                tasks.insert(solve.task_id)
                    && gateway_tasks.insert(solve.gateway_task_id.clone())
                    && problems.insert(solve.problem.record.problem_id.clone())
                    && runs.insert(solve.run.run_id.clone())
                    && solutions.insert(solve.solution.solution_id.clone()),
                "fixture identities must be unique across visible and denied rows"
            );
        }
        let authority = &self.visible[0].problem.record.authority;
        ensure!(
            authority.work_context.is_some()
                && self
                    .visible
                    .iter()
                    .all(
                        |s| s.problem.record.authority.principal_id == authority.principal_id
                            && s.problem.record.authority.work_context == authority.work_context
                    ),
            "visible corpus requires one isolated stable owner and Work Context"
        );
        ensure!(
            self.visible
                .windows(2)
                .all(|p| (p[0].run.created_at, p[0].task_id) < (p[1].run.created_at, p[1].task_id)),
            "visible corpus must declare independent catalog order"
        );
        let first = &self.visible[0];
        let last = &self.visible[OPTIMIZATION_INDEX_PAGE_SIZE - 1];
        ensure!(
            self.denied
                .iter()
                .any(
                    |s| (first.run.created_at, first.task_id) < (s.run.created_at, s.task_id)
                        && (s.run.created_at, s.task_id) < (last.run.created_at, last.task_id)
                ),
            "denied corpus must interleave before the first full catalog page ends"
        );
        let mut usage_order: Vec<_> = self.visible.iter().map(|s| s.task_id).collect();
        usage_order.sort();
        ensure!(
            self.denied.iter().any(|s| usage_order[0] < s.task_id
                && s.task_id < usage_order[OPTIMIZATION_USAGE_PAGE_SIZE - 1]),
            "denied corpus must interleave before the first full usage page ends"
        );
        Ok(())
    }
}
