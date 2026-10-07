use serde_json::json;
use std::collections::BTreeSet;
use veoveo_optimization_mcp::{contract::*, task_records::OptimizationTaskRequest};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_types::TaskId;

pub fn owner(tenant: Option<&str>, principal: &str, context: &str, labels: &[&str]) -> TaskOwner {
    serde_json::from_value(json!({
        "principal_key":principal,"principal_kind":"service","issuer":"https://optimization.test",
        "subject":principal,"profile":"operator","tenant_key":tenant,"data_labels":labels,
        "authority":{"work_context":context,"tenant":tenant.unwrap_or("installation"),
            "membership":"contributor","policy_revision":"test-1",
            "output_policy":{"owner":{"kind":"principal","id":principal}},
            "provenance":{"mode":"automated"}}
    }))
    .unwrap()
}

pub fn runtime(store: veoveo_platform_store::PlatformStore, worker: &str) -> TaskRuntime {
    veoveo_optimization_mcp::task_catalog::OptimizationTaskContributions::bind(TaskRuntime::new(
        store,
        "optimization",
        worker,
    ))
    .unwrap()
}

pub struct Row {
    pub task: TaskId,
    pub problem: ProblemId,
    pub run: RunId,
    pub solution: OptimizationSolutionUri,
}

pub async fn create(runtime: &TaskRuntime, owner: &TaskOwner, number: u64) -> Row {
    let task: TaskId = format!("0195dabe-7777-7abc-8def-{number:012x}")
        .parse()
        .unwrap();
    let problem = ProblemId::parse(format!("problem-{task}")).unwrap();
    let run = RunId::parse(format!("run-{task}")).unwrap();
    let solution = SolutionId::parse(format!("solution-{task}")).unwrap();
    let solution = OptimizationSolutionUri::new(solution).unwrap();
    // A retained record fixture; neither staging nor the GPU solver is executed.
    let request: OptimizationTaskRequest = serde_json::from_value(json!({
        "kind":"solve_convex",
        "common":{
            "problemId":problem,"runId":run,"family":"convex",
            "profileUri":"optimization://profile/balanced","submittedAt":"2026-09-28T00:00:00Z",
            "prepared":{"path":"fixture/prepared.json","digestSha256":"a".repeat(64),"bytes":1},
            "artifactWriteCapability":{
                "capabilityId":task,"secret":"s".repeat(32),"taskId":task,"expiresAt":"2026-10-01T00:00:00Z"
            }
        },
        "input":{
            "problem":{"source":"inline","problem":{
                "version":CONVEX_PROBLEM_VERSION,"kind":"linear_program",
                "variables":[{"variableId":"x","kind":"continuous","bounds":{"lower":0.0}}],
                "objective":{"direction":"minimize","linearTerms":[{"variableId":"x","coefficient":1.0}]}
            }},
            "policy":{"profileUri":"optimization://profile/balanced"}
        }
    })).unwrap();
    runtime
        .create(CreateTask {
            task_id: task,
            owner: owner.clone(),
            server: runtime.server().to_owned(),
            task_type: request.task_type(),
            request: serde_json::to_value(request).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    let artifact_id = veoveo_artifact_contract::ArtifactId::new();
    let artifact: veoveo_artifact_contract::ArtifactMetadata = serde_json::from_value(json!({
        "artifactId":artifact_id,"artifactUri":veoveo_artifact_contract::ArtifactUri::presented(&uris::SCHEME, artifact_id),
        "byteLen":1,"mimeType":"application/json","createdAt":"2026-09-28T00:00:00Z"
    }))
    .unwrap();
    let solution_artifact_id = veoveo_artifact_contract::ArtifactId::new();
    let mut solution_metadata = serde_json::to_value(&artifact).unwrap();
    solution_metadata["artifactId"] = serde_json::to_value(solution_artifact_id).unwrap();
    solution_metadata["artifactUri"] = serde_json::to_value(
        veoveo_artifact_contract::ArtifactUri::presented(&uris::SCHEME, solution_artifact_id),
    )
    .unwrap();
    let solution_artifact: veoveo_artifact_contract::ArtifactMetadata =
        serde_json::from_value(solution_metadata).unwrap();
    let output: OptimizationToolOutput = serde_json::from_value(json!({
        "runUri":OptimizationRunUri::new(run.clone()).unwrap(),"problemUri":OptimizationProblemUri::new(problem.clone()).unwrap(),"resultUri":solution,
        "family":"convex","feasibility":"feasible","termination":"optimal",
        "summary":{"family":"convex","quality":{"provenOptimal":true}},
        "problemArtifact":artifact,"solutionArtifact":solution_artifact
    })).unwrap();
    let result = veoveo_mcp_contract::hosting::product_result(
        "Fixture result ready",
        rmcp::model::Resource::new(solution.as_str(), "Solution"),
        &output,
    )
    .unwrap();
    runtime
        .claim(task, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    runtime
        .transition(
            task,
            veoveo_task_runtime::mcp_task_completion("fixture result", result).unwrap(),
        )
        .await
        .unwrap();
    Row {
        task,
        problem,
        run,
        solution,
    }
}
