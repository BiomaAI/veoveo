//! Typed public reads, bounded and compared with the operator's independent corpus.
use super::{
    cleanup::Journal,
    fixture::{Corpus, Solve},
};
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rmcp::{
    Peer, RoleClient, ServiceError,
    model::{GetTaskParams, ReadResourceRequestParams, ResourceContents, TaskPayload},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, time::Duration};
use veoveo_mcp_contract::UsageReport;
use veoveo_optimization_mcp::contract::*;
use veoveo_testing_support::{installed::knowledge as installed, lifecycle::owner};
use veoveo_types::{CanonicalTaskId, ResourceAddress, TaskId};

#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Observation {
    ReadIntent {
        resource: OptimizationResource,
    },
    ArtifactReadIntent {
        resource: OptimizationResource,
    },
    DeniedReadIntent {
        resource: OptimizationResource,
    },
    TaskReadIntent {
        task_id: TaskId,
        gateway_task_id: CanonicalTaskId,
    },
    SolvePassed {
        task_id: TaskId,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProblemEntry {
    problem_uri: OptimizationProblemUri,
    family: ProblemFamily,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProblemPage {
    problems: Vec<ProblemEntry>,
    limit: usize,
    next_cursor: Option<OptimizationIndexCursor>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunEntry {
    run_uri: OptimizationRunUri,
    family: ProblemFamily,
    phase: RunPhase,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunPage {
    runs: Vec<RunEntry>,
    limit: usize,
    next_cursor: Option<OptimizationIndexCursor>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SolutionEntry {
    result_uri: OptimizationSolutionUri,
    family: ProblemFamily,
    feasibility: SolutionFeasibility,
    termination: SolverTermination,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SolutionPage {
    solutions: Vec<SolutionEntry>,
    limit: usize,
    next_cursor: Option<OptimizationIndexCursor>,
}

pub fn check_page(
    expected: &[TaskId],
    actual: &[TaskId],
    offset: usize,
    limit: usize,
    next: Option<TaskId>,
) -> Result<()> {
    ensure!(
        limit == OPTIMIZATION_INDEX_PAGE_SIZE && actual.len() <= limit,
        "catalog limit or size changed"
    );
    let end = (offset + limit).min(expected.len());
    ensure!(
        offset <= expected.len() && actual == &expected[offset..end],
        "catalog membership or independent order differs"
    );
    ensure!(
        next == if end < expected.len() {
            actual.last().copied()
        } else {
            None
        },
        "catalog cursor skipped, cycled or terminated early"
    );
    Ok(())
}
async fn read<T: DeserializeOwned>(
    peer: &Peer<RoleClient>,
    resource: OptimizationResource,
    journal: &Journal,
) -> Result<T> {
    owner::check_effect()?;
    journal.append(&Observation::ReadIntent {
        resource: resource.clone(),
    })?;
    let uri = resource.to_uri()?;
    peer.clear_response_cache().await;
    tokio::time::timeout(Duration::from_secs(15), installed::read(peer, &uri)).await?
}
async fn catalogs(
    peer: &Peer<RoleClient>,
    corpus: &Corpus,
    journal: &Journal,
    alternate: bool,
) -> Result<()> {
    let selected = if alternate {
        &[][..]
    } else {
        corpus.visible.as_slice()
    };
    let expected: Vec<_> = selected.iter().map(|s| s.task_id).collect();
    for collection in [
        OptimizationCollection::Problems,
        OptimizationCollection::Runs,
        OptimizationCollection::Solutions,
    ] {
        let mut cursor = None;
        let mut offset = 0;
        for _ in 0..=2 {
            let resource = OptimizationResource::Collection(OptimizationCollectionUri::new(
                collection,
                cursor.clone(),
            )?);
            let (actual, limit, next) = match collection {
                OptimizationCollection::Problems => {
                    let page: ProblemPage = read(peer, resource, journal).await?;
                    let ids = page
                        .problems
                        .iter()
                        .map(|entry| {
                            selected
                                .iter()
                                .find(|s| {
                                    s.problem.record.problem_uri == entry.problem_uri
                                        && s.problem.record.family == entry.family
                                })
                                .map(|s| s.task_id)
                                .ok_or_else(|| anyhow::anyhow!("unexpected problem catalog entry"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    (ids, page.limit, page.next_cursor)
                }
                OptimizationCollection::Runs => {
                    let page: RunPage = read(peer, resource, journal).await?;
                    let ids = page
                        .runs
                        .iter()
                        .map(|entry| {
                            selected
                                .iter()
                                .find(|s| {
                                    s.run.run_uri == entry.run_uri
                                        && s.run.family == entry.family
                                        && s.run.phase == entry.phase
                                })
                                .map(|s| s.task_id)
                                .ok_or_else(|| anyhow::anyhow!("unexpected run catalog entry"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    (ids, page.limit, page.next_cursor)
                }
                OptimizationCollection::Solutions => {
                    let page: SolutionPage = read(peer, resource, journal).await?;
                    let ids = page
                        .solutions
                        .iter()
                        .map(|entry| {
                            selected
                                .iter()
                                .find(|s| {
                                    s.output.result_uri == entry.result_uri
                                        && s.output.family == entry.family
                                        && s.solution.feasibility == entry.feasibility
                                        && s.solution.termination == entry.termination
                                })
                                .map(|s| s.task_id)
                                .ok_or_else(|| anyhow::anyhow!("unexpected solution catalog entry"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    (ids, page.limit, page.next_cursor)
                }
            };
            check_page(
                &expected,
                &actual,
                offset,
                limit,
                next.as_ref().map(|c| c.task_id()),
            )?;
            if let Some(next) = next.as_ref() {
                let last = &selected[offset + actual.len() - 1];
                ensure!(
                    next.collection() == collection && next.created_at() == last.run.created_at,
                    "catalog cursor context or timestamp differs"
                );
            }
            offset += actual.len();
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        ensure!(
            cursor.is_none() && offset == expected.len(),
            "catalog page budget exhausted"
        );
    }
    let mut expected: Vec<_> = selected.iter().map(|s| s.task_id).collect();
    expected.sort();
    let mut cursor = None;
    let mut offset = 0;
    for _ in 0..=2 {
        let page: OptimizationUsagePage = read(
            peer,
            OptimizationResource::Usage(OptimizationUsageIndexUri::new(cursor.as_ref())),
            journal,
        )
        .await?;
        let ids: Vec<_> = page.usage().iter().map(|e| e.task_id()).collect();
        check_page(
            &expected,
            &ids,
            offset,
            OPTIMIZATION_USAGE_PAGE_SIZE,
            page.next_cursor().map(|c| c.after()),
        )?;
        offset += ids.len();
        cursor = page.next_cursor().cloned();
        if cursor.is_none() {
            break;
        }
    }
    ensure!(
        cursor.is_none() && offset == expected.len(),
        "usage page budget exhausted"
    );
    Ok(())
}

pub fn check_exact(
    solve: &Solve,
    problem: &OptimizationProblemResource,
    run: &OptimizationRunRecord,
    solution: &OptimizationSolution,
    verification: &VerificationReport,
    output: &OptimizationToolOutput,
    usage: &UsageReport,
) -> Result<()> {
    ensure!(
        problem == &solve.problem
            && run == &solve.run
            && solution == &solve.solution
            && verification == &solve.solution.verification
            && output == &solve.output
            && usage == &solve.usage,
        "exact resource, output, verification, provenance or usage differs"
    );
    Ok(())
}
async fn bytes(
    peer: &Peer<RoleClient>,
    artifact: &veoveo_artifact_contract::ArtifactMetadata,
    journal: &Journal,
) -> Result<Vec<u8>> {
    let resource = OptimizationResource::Artifact(artifact.artifact_id());
    journal.append(&Observation::ArtifactReadIntent {
        resource: resource.clone(),
    })?;
    owner::check_effect()?;
    let uri = resource.to_uri()?;
    peer.clear_response_cache().await;
    let response = tokio::time::timeout(
        Duration::from_secs(15),
        peer.read_resource(ReadResourceRequestParams::new(uri.as_str())),
    )
    .await??;
    let [
        ResourceContents::BlobResourceContents {
            uri: actual, blob, ..
        },
    ] = response.contents.as_slice()
    else {
        anyhow::bail!("expected single Artifact blob");
    };
    ensure!(
        actual == uri.as_str() && blob.len() <= 350 * 1024,
        "Artifact URI or encoded byte budget differs"
    );
    let bytes = STANDARD.decode(blob)?;
    ensure!(
        bytes.len() as u64 == artifact.byte_len && bytes.len() <= 256 * 1024,
        "Artifact byte count differs"
    );
    Ok(bytes)
}
async fn exact(peer: &Peer<RoleClient>, solve: &Solve, journal: &Journal) -> Result<()> {
    let problem = read(
        peer,
        OptimizationResource::Problem(solve.problem.record.problem_uri.clone()),
        journal,
    )
    .await?;
    let run = read(
        peer,
        OptimizationResource::Run(solve.run.run_uri.clone()),
        journal,
    )
    .await?;
    let solution = read(
        peer,
        OptimizationResource::Solution(solve.solution.solution_uri.clone()),
        journal,
    )
    .await?;
    let verification = read(
        peer,
        OptimizationResource::SolutionVerification(solve.solution.solution_id.clone()),
        journal,
    )
    .await?;
    let usage = read(
        peer,
        OptimizationResource::TaskUsage(OptimizationTaskUsageUri::new(solve.task_id)?),
        journal,
    )
    .await?;
    journal.append(&Observation::TaskReadIntent {
        task_id: solve.task_id,
        gateway_task_id: solve.gateway_task_id.clone(),
    })?;
    owner::check_effect()?;
    let task = tokio::time::timeout(
        Duration::from_secs(15),
        peer.get_task(GetTaskParams::new(solve.gateway_task_id.to_string())),
    )
    .await??;
    ensure!(
        CanonicalTaskId::parse(&task.task.task.task_id)? == solve.gateway_task_id,
        "public Task route identity differs"
    );
    ensure!(
        serde_json::to_vec(&task)?.len() <= 64 * 1024,
        "Task result exceeded read budget"
    );
    let TaskPayload::Completed { result } = task.task.payload else {
        anyhow::bail!("fixture Task is not completed");
    };
    let result: rmcp::model::CallToolResult =
        serde_json::from_value(serde_json::Value::Object(result))?;
    ensure!(
        result.is_error != Some(true),
        "fixture Task result is error"
    );
    let output: OptimizationToolOutput = serde_json::from_value(
        result
            .structured_content
            .ok_or_else(|| anyhow::anyhow!("missing Task output"))?,
    )?;
    check_exact(
        solve,
        &problem,
        &run,
        &solution,
        &verification,
        &output,
        &usage,
    )?;
    let p = bytes(peer, &output.problem_artifact, journal).await?;
    let s = bytes(peer, &output.solution_artifact, journal).await?;
    ensure!(
        p == serde_json::to_vec(&solve.problem)? && s == serde_json::to_vec(&solve.solution)?,
        "canonical Artifact bytes differ from independently selected products"
    );
    Ok(())
}
async fn denied(peer: &Peer<RoleClient>, solve: &Solve, journal: &Journal) -> Result<()> {
    for resource in [
        OptimizationResource::Problem(solve.problem.record.problem_uri.clone()),
        OptimizationResource::Run(solve.run.run_uri.clone()),
        OptimizationResource::Solution(solve.solution.solution_uri.clone()),
        OptimizationResource::SolutionVerification(solve.solution.solution_id.clone()),
        OptimizationResource::TaskUsage(OptimizationTaskUsageUri::new(solve.task_id)?),
        OptimizationResource::Artifact(solve.output.problem_artifact.artifact_id()),
        OptimizationResource::Artifact(solve.output.solution_artifact.artifact_id()),
    ] {
        owner::check_effect()?;
        journal.append(&Observation::DeniedReadIntent {
            resource: resource.clone(),
        })?;
        peer.clear_response_cache().await;
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            peer.read_resource(ReadResourceRequestParams::new(resource.to_uri()?.as_str())),
        )
        .await?;
        // Transport failure, timeout and decoding failure never establish denial.
        ensure!(
            matches!(result, Err(ServiceError::McpError(ref error)) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS),
            "expected current resource admission refusal"
        );
    }
    Ok(())
}
pub async fn exercise(
    primary: &Peer<RoleClient>,
    alternate: &Peer<RoleClient>,
    corpus: &Corpus,
    journal: &Journal,
) -> Result<()> {
    catalogs(primary, corpus, journal, false).await?;
    catalogs(alternate, corpus, journal, true).await?;
    let mut seen = BTreeSet::new();
    for solve in &corpus.visible {
        ensure!(
            seen.insert(solve.task_id),
            "duplicate exact fixture selection"
        );
        exact(primary, solve, journal).await?;
        journal.append(&Observation::SolvePassed {
            task_id: solve.task_id,
        })?;
    }
    for solve in &corpus.denied {
        denied(primary, solve, journal).await?;
    }
    for solve in corpus.visible.iter().take(2) {
        denied(alternate, solve, journal).await?;
    }
    // Stable fixture prerequisite: final full discovery also detects concurrent additions/removal.
    catalogs(primary, corpus, journal, false).await?;
    Ok(())
}
