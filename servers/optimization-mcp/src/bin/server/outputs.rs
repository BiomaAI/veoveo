use std::collections::BTreeMap;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::Serialize;
use serde_json::json;
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_optimization_mcp::{
    domain::{
        ConvexOutputPolicy, MilpOutputPolicy, OptimizationProblemResource, OptimizationSolution,
        OptimizationToolOutput, OptimizationToolSummary, ProblemFamily, RouteOutputPolicy,
        SolutionDetail, VerificationReport, VerifySolutionOutput,
    },
    problem_store::PreparedProblem,
    state::TaskOwner,
    uris,
};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
use veoveo_task_runtime::TaskId;

use super::app_state::AppState;

pub(super) enum RequestedArtifacts<'a> {
    Routing(&'a RouteOutputPolicy),
    Convex(&'a ConvexOutputPolicy),
    Milp(&'a MilpOutputPolicy),
}

pub(super) async fn solution_result(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    owner: &TaskOwner,
    prepared: &PreparedProblem,
    solution: OptimizationSolution,
    requested: RequestedArtifacts<'_>,
) -> anyhow::Result<CallToolResult> {
    let problem = prepared.resource();
    let problem_artifact = store_json_artifact(
        state,
        capability,
        owner,
        problem,
        "problem",
        format!("{}.problem.json", problem.record.problem_id),
        "canonical_problem",
    )
    .await?;
    let solution_artifact = store_json_artifact(
        state,
        capability,
        owner,
        &solution,
        "solution",
        format!("{}.solution.json", solution.solution_id),
        "canonical_solution",
    )
    .await?;
    let mut artifacts = Vec::new();
    match requested {
        RequestedArtifacts::Routing(policy) if policy.include_route_table_artifact => {
            artifacts.push(
                store_bytes_artifact(
                    state,
                    capability,
                    owner,
                    route_table(&solution)?.into_bytes(),
                    "route-table",
                    format!("{}.routes.csv", solution.solution_id),
                    "text/csv",
                    "route_table",
                )
                .await?,
            );
        }
        RequestedArtifacts::Convex(policy) => {
            if policy.retain_warm_start {
                artifacts.push(
                    store_json_artifact(
                        state,
                        capability,
                        owner,
                        &solution_variables(&solution)?,
                        "warm-start",
                        format!("{}.warm-start.json", solution.solution_id),
                        "warm_start",
                    )
                    .await?,
                );
            }
        }
        RequestedArtifacts::Milp(policy) => {
            if policy.retain_warm_start {
                artifacts.push(
                    store_json_artifact(
                        state,
                        capability,
                        owner,
                        &solution_variables(&solution)?,
                        "warm-start",
                        format!("{}.warm-start.json", solution.solution_id),
                        "warm_start",
                    )
                    .await?,
                );
            }
            if policy.retain_incumbents {
                artifacts.push(
                    store_json_artifact(
                        state,
                        capability,
                        owner,
                        &solution_incumbents(&solution)?,
                        "incumbents",
                        format!("{}.incumbents.json", solution.solution_id),
                        "incumbents",
                    )
                    .await?,
                );
            }
        }
        RequestedArtifacts::Routing(_) => {}
    }

    let family = problem.record.family;
    let summary = match &solution.detail {
        SolutionDetail::Routing { summaries, .. } => OptimizationToolSummary::Routing {
            cases: summaries.clone(),
        },
        SolutionDetail::Convex { quality, .. } => OptimizationToolSummary::Convex {
            quality: quality.clone(),
        },
        SolutionDetail::Milp { quality, .. } => OptimizationToolSummary::Milp {
            quality: quality.clone(),
        },
    };
    let output = OptimizationToolOutput {
        run_uri: veoveo_optimization_mcp::domain::OptimizationRunUri::parse(uris::run_uri(
            &solution.run_id,
        ))?,
        problem_uri: solution.problem_uri.clone(),
        solution_uri: solution.solution_uri.clone(),
        family,
        feasibility: solution.feasibility,
        termination: solution.termination,
        summary,
        problem_artifact,
        solution_artifact,
        artifacts,
    };
    record_usage(state, task_id, problem, &solution).await?;

    let mut content = vec![ContentBlock::text(format!(
        "{} completed with {:?} termination; solution {} is {:?} and independently verified: {}",
        family_name(family),
        output.termination,
        solution.solution_id,
        output.feasibility,
        solution.verification.verified
    ))];
    content.extend([
        json_link(
            output.problem_uri.as_str(),
            format!("problem {}", problem.record.problem_id),
            "Immutable normalized optimization problem.",
        ),
        json_link(
            output.run_uri.as_str(),
            format!("run {}", solution.run_id),
            "Durable cuOpt execution record and engine provenance.",
        ),
        json_link(
            output.solution_uri.as_str(),
            format!("solution {}", solution.solution_id),
            "Verified optimization solution and quality metrics.",
        ),
        artifact_link(
            &output.problem_artifact,
            "canonical problem JSON",
            "Canonical normalized problem bytes.",
        ),
        artifact_link(
            &output.solution_artifact,
            "canonical solution JSON",
            "Canonical verified solution bytes.",
        ),
    ]);
    content.extend(output.artifacts.iter().map(|artifact| {
        artifact_link(
            artifact,
            artifact
                .filename
                .as_deref()
                .unwrap_or("optimization artifact"),
            "Optional optimization evidence artifact.",
        )
    }));
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

pub(super) async fn verification_result(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    owner: &TaskOwner,
    solution: &OptimizationSolution,
    report: VerificationReport,
) -> anyhow::Result<CallToolResult> {
    let report_artifact = store_json_artifact(
        state,
        capability,
        owner,
        &report,
        "verification",
        format!("{}.verification.json", solution.solution_id),
        "verification_report",
    )
    .await?;
    let output = VerifySolutionOutput {
        solution_uri: solution.solution_uri.clone(),
        report: report.clone(),
        report_artifact: Some(report_artifact.clone()),
    };
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(format!(
            "solution {} independently verified: {}; {} finding(s)",
            solution.solution_id,
            report.verified,
            report.findings.len()
        )),
        json_link(
            &uris::solution_verification_uri(&solution.solution_id),
            format!("published verification for {}", solution.solution_id),
            "The verification report embedded when the immutable solution was published.",
        ),
        artifact_link(
            &report_artifact,
            "verification report JSON",
            "Independent verification evidence.",
        ),
    ]);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

async fn store_json_artifact<T: Serialize>(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    owner: &TaskOwner,
    value: &T,
    idempotency_suffix: &str,
    filename: String,
    kind: &str,
) -> anyhow::Result<ArtifactMetadata> {
    store_bytes_artifact(
        state,
        capability,
        owner,
        serde_json::to_vec(value)?,
        idempotency_suffix,
        filename,
        "application/json",
        kind,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn store_bytes_artifact(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    owner: &TaskOwner,
    bytes: Vec<u8>,
    idempotency_suffix: &str,
    filename: String,
    mime_type: &str,
    kind: &str,
) -> anyhow::Result<ArtifactMetadata> {
    let mut put = ArtifactPut::new(bytes);
    put.mime_type = Some(mime_type.to_owned());
    put.filename = Some(filename);
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        work_context: Some(owner.authority.work_context.clone()),
        ..Default::default()
    };
    put.metadata = json!({"kind": kind});
    Ok(state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("optimization:{idempotency_suffix}"))?,
            put,
        )
        .await?
        .without_download_url())
}

fn json_link(uri: &str, title: String, description: &str) -> ContentBlock {
    ContentBlock::resource_link(
        Resource::new(uri.to_owned(), title.clone())
            .with_title(title)
            .with_description(description.to_owned())
            .with_mime_type("application/json"),
    )
}

fn artifact_link(artifact: &ArtifactMetadata, title: &str, description: &str) -> ContentBlock {
    let mut resource = Resource::new(artifact.artifact_uri.clone(), title.to_owned())
        .with_title(title.to_owned())
        .with_description(description.to_owned());
    if let Some(mime_type) = &artifact.mime_type {
        resource = resource.with_mime_type(mime_type.clone());
    }
    ContentBlock::resource_link(resource)
}

fn solution_variables(
    solution: &OptimizationSolution,
) -> anyhow::Result<&Vec<veoveo_optimization_mcp::domain::VariableValue>> {
    match &solution.detail {
        SolutionDetail::Convex { variables, .. } | SolutionDetail::Milp { variables, .. } => {
            Ok(variables)
        }
        SolutionDetail::Routing { .. } => {
            anyhow::bail!("routing solutions do not contain mathematical warm starts")
        }
    }
}

fn solution_incumbents(
    solution: &OptimizationSolution,
) -> anyhow::Result<&Vec<veoveo_optimization_mcp::domain::IncumbentSummary>> {
    match &solution.detail {
        SolutionDetail::Milp { incumbents, .. } => Ok(incumbents),
        _ => anyhow::bail!("only MILP solutions contain incumbent histories"),
    }
}

fn route_table(solution: &OptimizationSolution) -> anyhow::Result<String> {
    let SolutionDetail::Routing { routes, .. } = &solution.detail else {
        anyhow::bail!("route table requested for a non-routing solution");
    };
    let mut output =
        "case_id,vehicle_id,sequence,order_id,location_id,node_kind,arrival,departure,cumulative_cost\n"
            .to_owned();
    for route in routes {
        for stop in &route.stops {
            output.push_str(&format!(
                "{},{},{},{},{},{:?},{},{},{}\n",
                route.case_id.as_ref().map_or("", |id| id.as_str()),
                route.vehicle_id,
                stop.sequence,
                stop.order_id.as_ref().map_or("", |id| id.as_str()),
                stop.location_id,
                stop.node_kind,
                stop.arrival.get(),
                stop.departure.get(),
                stop.cumulative_cost.get(),
            ));
        }
    }
    Ok(output)
}

async fn record_usage(
    state: &AppState,
    task_id: &str,
    problem: &OptimizationProblemResource,
    solution: &OptimizationSolution,
) -> anyhow::Result<()> {
    let dimensions = &problem.record.dimensions;
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "optimization".to_owned(),
            source_id: solution.engine.gpu_uuid.clone(),
            provider_job_id: Some(solution.run_id.to_string()),
            model_id: format!("nvidia/cuopt:{}", solution.engine.version),
            kind: DomainUsageKind::Actual,
            quantity: Some(solution.timings.solve_seconds.get()),
            unit: Some("gpu_second".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                ("family".into(), json!(problem.record.family)),
                ("problem_id".into(), json!(problem.record.problem_id)),
                ("solution_id".into(), json!(solution.solution_id)),
                ("verified".into(), json!(solution.verification.verified)),
                ("feasibility".into(), json!(solution.feasibility)),
                ("termination".into(), json!(solution.termination)),
                ("locations".into(), json!(dimensions.locations)),
                ("orders".into(), json!(dimensions.orders)),
                ("vehicles".into(), json!(dimensions.vehicles)),
                ("variables".into(), json!(dimensions.variables)),
                ("constraints".into(), json!(dimensions.constraints)),
                ("nonzeros".into(), json!(dimensions.nonzeros)),
            ])),
        })
        .await?;
    Ok(())
}

fn family_name(family: ProblemFamily) -> &'static str {
    match family {
        ProblemFamily::Routing => "route optimization",
        ProblemFamily::RouteScenarios => "route-scenario optimization",
        ProblemFamily::Convex => "convex optimization",
        ProblemFamily::Milp => "MILP optimization",
    }
}
