//! Native controls never acquire installed credentials or execute a solver.
use super::*;
#[test]
fn installed_catalog_assertions_reject_missing_reordered_duplicate_and_bad_cursor() -> Result<()> {
    use veoveo_types::TaskId;
    let expected: Vec<TaskId> = (1..=101)
        .map(|n| format!("0195dabe-7777-7abc-8def-{n:012x}").parse().unwrap())
        .collect();
    reads::check_page(&expected, &expected[..100], 0, 100, Some(expected[99]))?;
    reads::check_page(&expected, &expected[100..], 100, 100, None)?;
    ensure!(reads::check_page(&expected, &expected[..99], 0, 100, Some(expected[98])).is_err());
    ensure!(reads::check_page(&expected, &expected[..100], 0, 100, None).is_err());
    ensure!(reads::check_page(&expected, &expected[..100], 0, 100, Some(expected[98])).is_err());
    let mut duplicate = expected[..100].to_vec();
    duplicate[99] = duplicate[98];
    ensure!(reads::check_page(&expected, &duplicate, 0, 100, Some(expected[99])).is_err());
    let mut reordered = expected[..100].to_vec();
    reordered.swap(0, 1);
    ensure!(reads::check_page(&expected, &reordered, 0, 100, Some(expected[99])).is_err());
    Ok(())
}

#[tokio::test]
async fn installed_read_owner_drop_closes_actual_sdk_handles() -> Result<()> {
    const MODE: &str = "VEOVEO_OPTIMIZATION_READ_CLEANUP_CONTROL";
    const ROOT: &str = "VEOVEO_OPTIMIZATION_READ_CLEANUP_ROOT";
    if let Ok(mode) = std::env::var(MODE) {
        use rmcp::{ClientServiceExt, ServiceExt};
        use std::{
            fs,
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
        };
        let root = std::path::PathBuf::from(std::env::var_os(ROOT).unwrap());
        let journal = Journal::create(&root.join("report.jsonl"))?;
        let attempts = Arc::new(AtomicUsize::new(0));
        let (closed_tx, mut closed_rx) = tokio::sync::mpsc::unbounded_channel();
        let result: Result<()> = owner::run(async {
            let (cleanup, _registration) = Cleanup::register(journal)?;
            let mut admitted = cleanup.admission().await;
            let interrupted = mode == "interrupted";
            let failed = mode == "failed";
            for index in 0..2 {
                let (server_io, client_io) = tokio::io::duplex(8192);
                struct Source;
                impl rmcp::ServerHandler for Source {}
                let server = tokio::spawn(async move { Source.serve(server_io).await });
                let client = ()
                    .serve_with_lifecycle(
                        client_io,
                        rmcp::ClientLifecycleMode::Discover {
                            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                        },
                    )
                    .await?;
                let counted = Arc::clone(&attempts);
                admitted.retain(index, async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    if interrupted && index == 0 {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    client.cancel().await?;
                    ensure!(
                        !(failed && index == 0),
                        "controlled missing SDK close acknowledgement"
                    );
                    Ok(())
                });
                let server = server.await??;
                let closed_tx = closed_tx.clone();
                tokio::spawn(async move {
                    let _ = closed_tx.send(server.waiting().await.is_ok());
                });
            }
            drop(admitted);
            if failed {
                ensure!(
                    cleanup.close().await.is_err(),
                    "controlled failure disappeared"
                );
                ensure!(
                    cleanup.close().await.is_err(),
                    "empty consumed handle falsely settled"
                );
                return Ok(());
            }
            let pid = std::process::id();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(80)).await;
                let status = tokio::process::Command::new("kill")
                    .args(["-INT", &pid.to_string()])
                    .status()
                    .await
                    .unwrap();
                assert!(status.success());
            });
            if interrupted {
                cleanup.close().await?;
            }
            std::future::pending::<Result<()>>().await
        })
        .await;
        ensure!(result.is_err(), "global drop or failed close became a pass");
        ensure!(
            attempts.load(Ordering::SeqCst) == 2,
            "original SDK consuming close was retried or omitted"
        );
        for _ in 0..2 {
            ensure!(
                tokio::time::timeout(Duration::from_secs(1), closed_rx.recv()).await? == Some(true),
                "actual SDK transport survived cleanup"
            );
        }
        let report = fs::read_to_string(root.join("report.jsonl"))?;
        let last: serde_json::Value = serde_json::from_str(report.lines().last().unwrap())?;
        ensure!(
            last["phase"] == "cleanup" && last["operation"] == "unfinished",
            "global drop lost unfinished operation facts"
        );
        ensure!(
            last["clients"][0] == if mode == "failed" { "failed" } else { "passed" },
            "sticky close outcome changed"
        );
        ensure!(
            last["clients"][1] == "passed",
            "other actual client was skipped after first close failure"
        );
        return Ok(());
    }
    for mode in ["global", "interrupted", "failed"] {
        let directory = tempfile::tempdir()?;
        std::fs::create_dir(directory.path().join("groups"))?;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "installed_read_owner_drop_closes_actual_sdk_handles",
                "--nocapture",
            ])
            .env(MODE, mode)
            .env(ROOT, directory.path())
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path().join("groups"))
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2");
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(15)).await?;
        ensure!(
            output.status.success(),
            "actual SDK owner-drop native control failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn native_solve(number: u64) -> Result<fixture::Solve> {
    // This models wire/fixture admission only. No solver, CUDA or provider is executed.
    use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, ArtifactUri};
    use veoveo_optimization_mcp::{
        contract::*,
        executor::ExecutorMathematicalSolution,
        solution_builder::{SolutionContext, build_convex_solution},
    };
    let task_id: veoveo_types::TaskId = format!("0195dabe-7777-7abc-8def-{number:012x}").parse()?;
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")?.to_utc();
    let authority: OptimizationAuthority = serde_json::from_value(serde_json::json!({
        "principalId":"native#actor", "workContext":"native-fixture", "policyRevision":"native", "submittedAt":now
    }))?;
    let model: ConvexProblem = serde_json::from_value(
        serde_json::json!({"version":CONVEX_PROBLEM_VERSION,
        "kind":"linear_program", "variables":[{"variableId":"x","kind":"continuous","bounds":{"lower":0.0}}],
        "objective":{"direction":"minimize","linearTerms":[{"variableId":"x","coefficient":1.0}]}}),
    )?;
    let definition = OptimizationProblemDefinition::Convex {
        problem: model.clone(),
    };
    let id = ProblemId::new();
    let problem = OptimizationProblemResourceValue {
        record: OptimizationProblemRecordValue {
            problem_uri: OptimizationProblemUri::new(id.clone())?,
            problem_id: id,
            family: ProblemFamily::Convex,
            schema_version: CONVEX_PROBLEM_VERSION.into(),
            digest_sha256: definition_digest(&definition)?,
            dimensions: definition.dimensions()?,
            authority: authority.clone(),
            created_at: now,
        }
        .build()?,
        definition,
    }
    .build()?;
    let engine: EngineProvenance = serde_json::from_value(
        serde_json::json!({"name":"NVIDIA cuOpt", "version":CUOPT_STABLE_VERSION,
        "containerDigest":CUOPT_CONTAINER_DIGEST, "executorProtocol":EXECUTOR_PROTOCOL_VERSION,
        "gpuName":"native fixture metadata only", "gpuUuid":"native-fixture-not-hardware", "computeCapability":"fixture",
        "solverProfileUri":"optimization://profile/balanced"}),
    )?;
    let executor: ExecutorMathematicalSolution =
        serde_json::from_value(serde_json::json!({"family":"convex","status":"optimal",
        "primalSolution":[0.0], "primalObjective":0.0, "solveSeconds":0.0}))?;
    let solution = build_convex_solution(
        &model,
        &executor,
        SolutionContext {
            run_id: RunId::new(),
            problem_uri: problem.record.problem_uri.clone(),
            engine: engine.clone(),
            timings: RunTimings::default(),
            authority: authority.clone(),
            created_at: now,
        },
    )?;
    let run = OptimizationRunRecord {
        run_id: solution.run_id.clone(),
        run_uri: OptimizationRunUri::new(solution.run_id.clone())?,
        problem_uri: problem.record.problem_uri.clone(),
        family: ProblemFamily::Convex,
        phase: RunPhase::Completed,
        incumbent: None,
        solution_uri: Some(solution.solution_uri.clone()),
        engine,
        timings: RunTimings::default(),
        authority,
        created_at: now,
        updated_at: now,
    }
    .build()?;
    let artifact = |len| -> Result<ArtifactMetadata> {
        let id = ArtifactId::new();
        Ok(serde_json::from_value(
            serde_json::json!({"artifactId":id, "artifactUri":ArtifactUri::presented(&uris::SCHEME,id),
            "byteLen":len,"mimeType":"application/json","createdAt":now,
            "compliance":{"workContext":"native-fixture", "provenance":{"invocationMode":"automated", "producer":"native#actor", "policyRevision":"native"}}}),
        )?)
    };
    let output = OptimizationToolOutputValue {
        run_uri: run.run_uri.clone(),
        problem_uri: problem.record.problem_uri.clone(),
        result_uri: solution.solution_uri.clone(),
        family: ProblemFamily::Convex,
        feasibility: solution.feasibility,
        termination: solution.termination,
        summary: OptimizationToolSummary::Convex {
            quality: match &solution.detail {
                SolutionDetail::Convex { quality, .. } => quality.clone(),
                _ => unreachable!(),
            },
        },
        problem_artifact: artifact(serde_json::to_vec(&problem)?.len())?,
        solution_artifact: artifact(serde_json::to_vec(&solution)?.len())?,
        artifacts: vec![],
    }
    .build()?;
    let usage = veoveo_mcp_contract::UsageReport::new(task_id.to_string(), OptimizationTaskUsageUri::new(task_id)?.as_str()).with_records(vec![
        serde_json::from_value(serde_json::json!({"taskId":task_id.to_string(), "modelId":"cuopt", "kind":"actual", "recordedAt":now}))?]);
    Ok(fixture::Solve {
        gateway_task_id: veoveo_types::CanonicalTaskId::parse(format!("gtr_native_{number}"))?,
        task_id,
        problem,
        run,
        solution,
        output,
        usage,
    })
}

#[test]
fn installed_fixture_admits_independent_corpus_and_rejects_detached_or_missing_provenance()
-> Result<()> {
    let mut corpus = fixture::Corpus {
        schema: "veoveo.ai/optimization-consumer-fixture/v1".into(),
        visible: (1..=101)
            .map(|n| native_solve(n * 2))
            .collect::<Result<Vec<_>>>()?,
        denied: vec![native_solve(3)?],
    };
    corpus.admit()?;
    let solve = &corpus.visible[0];
    reads::check_exact(
        solve,
        &solve.problem,
        &solve.run,
        &solve.solution,
        &solve.solution.verification,
        &solve.output,
        &solve.usage,
    )?;
    let other = &corpus.visible[1];
    ensure!(
        reads::check_exact(
            solve,
            &other.problem,
            &solve.run,
            &solve.solution,
            &solve.solution.verification,
            &solve.output,
            &solve.usage
        )
        .is_err()
    );
    let mut wrong = solve.run.clone();
    wrong.engine.gpu_uuid = None;
    ensure!(
        reads::check_exact(
            solve,
            &solve.problem,
            &wrong,
            &solve.solution,
            &solve.solution.verification,
            &solve.output,
            &solve.usage
        )
        .is_err()
    );
    let original = corpus.visible[0].clone();
    corpus.visible[0].run.phase = veoveo_optimization_mcp::contract::RunPhase::Solving;
    ensure!(corpus.admit().is_err());
    corpus.visible[0] = original.clone();
    corpus.visible[0].task_id = corpus.visible[1].task_id;
    ensure!(corpus.admit().is_err());
    corpus.visible[0] = original.clone();
    corpus.visible[0].run.engine.gpu_uuid = None;
    ensure!(corpus.admit().is_err());
    corpus.visible[0] = original;
    corpus.visible.swap(0, 1);
    ensure!(corpus.admit().is_err());
    corpus.visible.swap(0, 1);
    corpus.denied[0] = corpus.visible[0].clone();
    ensure!(corpus.admit().is_err());
    Ok(())
}

#[test]
fn installed_private_fixture_and_journal_refuse_public_or_reused_paths() -> Result<()> {
    use std::{fs, path::Path};
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("fixture.json");
    fs::write(&input, "[]")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        fs::set_permissions(&input, fs::Permissions::from_mode(0o644))?;
        ensure!(fixture::private_json::<Vec<u8>>(&input, 64).is_err());
        fs::set_permissions(&input, fs::Permissions::from_mode(0o600))?;
        let link = directory.path().join("link");
        symlink(&input, &link)?;
        ensure!(fixture::private_json::<Vec<u8>>(&link, 64).is_err());
    }
    ensure!(fixture::private_json::<Vec<u8>>(&input, 1).is_err());
    let _: Vec<u8> = fixture::private_json(&input, 64)?;
    ensure!(Journal::create(Path::new("relative")).is_err());
    let path = directory.path().join("report.jsonl");
    let journal = Journal::create(&path)?;
    journal.append(&Record::Finished {
        passed: false,
        failure: Some(Failure::Deadline),
    })?;
    ensure!(Journal::create(&path).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(fs::metadata(&path)?.permissions().mode() & 0o077 == 0);
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(value["passed"] == false && value["failure"] == "deadline");
    Ok(())
}

#[test]
fn installed_coordinated_input_admits_only_control_and_nvidia_executor_without_effects()
-> Result<()> {
    use fixture::RecoveryInput;
    let valid = serde_json::json!({"pod":"optimization-mcp-old-123", "profiles":[
        {"container":"optimization-mcp", "deadlineSeconds":20},
        {"container":"cuopt-executor", "deadlineSeconds":25}
    ]});
    let input: RecoveryInput = serde_json::from_value(valid.clone())?;
    let admitted = input.admit()?;
    ensure!(admitted.pod == "optimization-mcp-old-123" && admitted.profiles.len() == 2);
    // Admission is synchronous and receives only private values; no installation,
    // client, command runner or mutation capability is acquired to validate them.
    let mut swapped = valid.clone();
    swapped["profiles"].as_array_mut().unwrap().swap(0, 1);
    serde_json::from_value::<RecoveryInput>(swapped)?.admit()?;
    for name in ["other-control", "server", "executor", ""] {
        let mut wrong = valid.clone();
        wrong["profiles"][0]["container"] = serde_json::json!(name);
        ensure!(serde_json::from_value::<RecoveryInput>(wrong).is_err());
    }
    for deadline in [0, 301] {
        let mut wrong = valid.clone();
        wrong["profiles"][1]["deadlineSeconds"] = serde_json::json!(deadline);
        ensure!(
            serde_json::from_value::<RecoveryInput>(wrong)?
                .admit()
                .is_err()
        );
    }
    let mut duplicate = valid.clone();
    duplicate["profiles"][1]["container"] = serde_json::json!("optimization-mcp");
    ensure!(
        serde_json::from_value::<RecoveryInput>(duplicate)?
            .admit()
            .is_err()
    );
    for pod in [
        "",
        "../other",
        "-bad",
        "bad-",
        "other namespace",
        "$(touch marker)",
    ] {
        let mut wrong = valid.clone();
        wrong["pod"] = serde_json::json!(pod);
        ensure!(
            serde_json::from_value::<RecoveryInput>(wrong)?
                .admit()
                .is_err()
        );
    }
    let mut wrong = valid.clone();
    wrong["pod"] = serde_json::json!("a".repeat(254));
    ensure!(
        serde_json::from_value::<RecoveryInput>(wrong)?
            .admit()
            .is_err()
    );
    for field in [
        "deployment",
        "namespace",
        "endpoint",
        "allowCpu",
        "restartCount",
    ] {
        let mut wrong = valid.clone();
        wrong[field] = serde_json::json!("not-admitted");
        ensure!(serde_json::from_value::<RecoveryInput>(wrong).is_err());
    }
    let mut wrong = valid.clone();
    wrong["profiles"][1]["resource"] = serde_json::json!("ordinary-server");
    ensure!(serde_json::from_value::<RecoveryInput>(wrong).is_err());
    let mut wrong = valid;
    wrong["profiles"].as_array_mut().unwrap().pop();
    ensure!(serde_json::from_value::<RecoveryInput>(wrong).is_err());
    Ok(())
}

#[test]
fn installed_coordinated_admission_keeps_target_identity_after_file_replacement() -> Result<()> {
    use std::io::Write;
    let root = tempfile::tempdir()?;
    let target_file = root.path().join("target.json");
    let original = serde_json::json!({
        "schema":"veoveo.ai/installation-target/v1",
        "kubernetes":{"context":"native-original", "namespace":"native-original"},
        "localBaseUrl":"http://127.0.0.1:18781", "publicBaseUrl":"https://native.example.test",
        "controlPlane":"gateway.json", "expectedDeployments":["optimization-mcp"], "minimumGpuShares":1,
        "operator":{"clientId":"native-client", "profile":"native", "workContext":"native-fixture",
            "comparisonContext":"native-other", "scopes":["native:use"]}
    });
    std::fs::write(&target_file, serde_json::to_vec(&original)?)?;
    let corpus = fixture::Corpus {
        schema: "veoveo.ai/optimization-consumer-fixture/v1".into(),
        visible: (1..=101)
            .map(|n| native_solve(n * 2))
            .collect::<Result<Vec<_>>>()?,
        denied: vec![native_solve(3)?],
    };
    // NamedTempFile creates owner-private files; these are synthetic local
    // admission fixtures, not installed tokens or GPU qualification products.
    let mut corpus_file = tempfile::NamedTempFile::new_in(root.path())?;
    corpus_file.write_all(&serde_json::to_vec(&corpus)?)?;
    let mut primary = tempfile::NamedTempFile::new_in(root.path())?;
    primary.write_all(b"native-primary-placeholder")?;
    let mut alternate = tempfile::NamedTempFile::new_in(root.path())?;
    alternate.write_all(b"native-alternate-placeholder")?;
    let mut input: fixture::Input = serde_json::from_value(serde_json::json!({
        "installation":{"installationTarget":target_file, "endpoint":"https://native.example.test/mcp/native",
            "callerTokenFile":primary.path(), "deployment":"optimization-mcp", "output":root.path().join("report.jsonl")},
        "corpusFile":corpus_file.path(), "alternateTokenFile":alternate.path(), "alternateContext":"native-other",
        "coordinatedReplacement":{"pod":"optimization-mcp-old", "profiles":[
            {"container":"optimization-mcp", "deadlineSeconds":20}, {"container":"cuopt-executor", "deadlineSeconds":20}
        ]}
    }))?;
    input.coordinated_replacement.as_ref().unwrap().admit()?;
    let (_, admitted_target) = input.admit()?;
    let mut redirected = original;
    redirected["kubernetes"]["context"] = serde_json::json!("native-redirected");
    redirected["kubernetes"]["namespace"] = serde_json::json!("native-redirected");
    let replacement = root.path().join("replacement.json");
    std::fs::write(&replacement, serde_json::to_vec(&redirected)?)?;
    std::fs::rename(&replacement, &target_file)?;
    // The counterfactual reload accepts this replacement despite the unchanged
    // public origin. Production passes admitted_target directly to the driver.
    let reloaded = input.installation.validate()?;
    ensure!(
        reloaded.kubernetes.context == "native-redirected"
            && reloaded.kubernetes.namespace == "native-redirected"
            && reloaded.public_base_url == admitted_target.public_base_url
    );
    admitted_target.validate()?;
    ensure!(
        admitted_target.kubernetes.context == "native-original"
            && admitted_target.kubernetes.namespace == "native-original"
    );
    ensure!(
        !root.path().join("report.jsonl").exists(),
        "local admission performed effect preparation"
    );
    Ok(())
}
