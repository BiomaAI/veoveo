use super::*;
const SMOKE_CSV: &str = "city,population,elevation_m,region\nQuito,2800000,2850,sierra\nGuayaquil,3100000,4,costa\nCuenca,640000,2560,sierra\nLoja,290000,2060,sierra\nManta,310000,6,costa\n";

pub(crate) async fn datasheet_mcp(conformance: &Path, artifact_service: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(artifact_service)?;

    let template_dir = Path::new("templates/python-mcp");
    if !template_dir.is_dir() {
        bail!("datasheet smoke must run from the repository root");
    }
    let image_project_dir = template_dir;
    if !image_project_dir.is_dir() {
        bail!("datasheet image-build project is missing");
    }
    run_checked(
        Path::new("uv"),
        [
            "sync".into(),
            "--project".into(),
            image_project_dir.as_os_str().to_os_string(),
            "--locked".into(),
        ],
        [],
    )?;
    let datasheet_bin = image_project_dir.join(".venv/bin/datasheet-mcp");
    assert_executable(&datasheet_bin)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18811u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("datasheet.log");
    let output_dir = tmpdir.join("outputs");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut datasheet_child = spawn_datasheet_smoke(
        &datasheet_bin,
        port,
        &base,
        &plane.url,
        &plane.platform,
        &log,
    )?;
    wait_for_http(&format!("{base}/datasheet/readyz")).await?;
    let health = reqwest::get(format!("{base}/datasheet/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!("{base}/datasheet/healthz"))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("datasheet untrusted Host status was {untrusted_host_status}, expected 421");
    }
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-datasheet-mcp"),
            ("mcp_path", "/datasheet/mcp"),
        ],
    )?;
    assert_http_status(&format!("{base}/datasheet/mcp"), StatusCode::UNAUTHORIZED).await?;

    let mcp_url = format!("{base}/datasheet/mcp");
    {
        let identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("media")?)?;

        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            conformance,
            &mcp_url,
            ["--scheme".into(), "datasheet".into(), "info".into()],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::Http { status: 401 },
        )
    }?;

    let info = run_datasheet_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: datasheet",
        "tool `column_stats`",
        "tool `preview_dataset`",
        "tool `profile_dataset`",
        "prompt `datasheet-profile-dataset`",
        "prompt `datasheet-report-review`",
        "template: datasheet://usage/task/{task_id}",
        "template: datasheet://reports{?cursor}",
        "template: datasheet://usage{?cursor}",
        "template: datasheet://artifact/{artifact_id}",
    ] {
        contains(&info, expected)?;
    }

    let resources = run_datasheet_mcp(conformance, &mcp_url, ["resources".into()])?;
    contains(&resources, "datasheet://reports")?;
    contains(&resources, "datasheet://usage")?;

    let prompt = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "prompt".into(),
            "datasheet-profile-dataset".into(),
            "--arguments".into(),
            r#"{"datasetUri":"artifact://01900000-0000-7000-8000-000000000001"}"#.into(),
        ],
    )?;
    contains(&prompt, "profile_dataset")?;

    let preview_args = serde_json::json!({"inlineCsv": SMOKE_CSV, "rows": 3}).to_string();
    let preview = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "preview_dataset".into(),
            "--arguments".into(),
            preview_args.into(),
        ],
    )?;
    contains(&preview, "previewed 3 of 5 row(s)")?;
    let previewed: Value = structured_from_output(&preview)?;
    if previewed.pointer("/rowCount").and_then(Value::as_i64) != Some(5) {
        bail!("preview output had wrong rowCount: {previewed}");
    }
    if previewed.pointer("/columns/1/name").and_then(Value::as_str) != Some("population") {
        bail!("preview output had wrong column order: {previewed}");
    }

    let stats_args = serde_json::json!({"inlineCsv": SMOKE_CSV, "column": "region"}).to_string();
    let stats = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "column_stats".into(),
            "--arguments".into(),
            stats_args.into(),
        ],
    )?;
    let stats: Value = structured_from_output(&stats)?;
    if stats.pointer("/distinctCount").and_then(Value::as_i64) != Some(2) {
        bail!("column_stats output had wrong distinctCount: {stats}");
    }
    if stats.pointer("/topValues/0/value").and_then(Value::as_str) != Some("sierra") {
        bail!("column_stats output had wrong top value: {stats}");
    }

    // The task-required tool must reject direct invocation with an in-band
    // tool error and no structured output.
    let direct_args = serde_json::json!({"inlineCsv": SMOKE_CSV}).to_string();
    let rejected = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "profile_dataset".into(),
            "--arguments".into(),
            direct_args.into(),
        ],
    )?;
    contains(
        &rejected,
        "`profile_dataset` must be called as an MCP Task. Resend the call with task parameters.",
    )?;
    not_contains(&rejected, "structured:")?;

    let profile_args = serde_json::json!({"inlineCsv": SMOKE_CSV, "artifact": true}).to_string();
    let profiled = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "task-call".into(),
            "--tool-name".into(),
            "profile_dataset".into(),
            "--arguments".into(),
            profile_args.into(),
        ],
    )?;
    let task_id = task_id_from_output(&profiled)?;
    contains(&profiled, "profiled 5 row(s) across 4 column(s)")?;
    contains(&profiled, "output: datasheet://artifact/")?;
    let profile_output: Value = structured_from_output(&profiled)?;
    if profile_output
        .pointer("/profile/rowCount")
        .and_then(Value::as_i64)
        != Some(5)
    {
        bail!("profile output had wrong rowCount: {profile_output}");
    }
    if profile_output
        .pointer("/profile/columns/2/histogram/0/count")
        .and_then(Value::as_i64)
        .is_none()
    {
        bail!("profile output had no elevation histogram: {profile_output}");
    }
    let artifact = admit_profile_artifact(&profile_output, &task_id)?;
    let artifact_id = artifact.artifact_id().to_string();
    let artifact_uri = artifact.artifact_uri.to_string();

    run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "artifact".into(),
            artifact_id.clone().into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    assert_output_file(&output_dir, "bin")?;

    // Artifact access stays principal- and tenant-scoped on the shared plane.
    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("datasheet")?)?;
        identity.work_context = veoveo_types::WorkContextId::parse("intruder-context")?;
        identity.subject = veoveo_mcp_contract::TokenSubject::parse("intruder")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable(
                "veoveo-artifact-service",
                "artifact-smoke",
            )?,
            &mcp_url,
            [
                "--scheme".into(),
                "datasheet".into(),
                "artifact".into(),
                artifact_id.clone().into(),
                "--output-dir".into(),
                tmpdir.join("denied-intruder").as_os_str().to_os_string(),
            ],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                "unknown artifact",
            ),
        )
    }?;
    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("datasheet")?)?;
        identity.tenant = veoveo_types::TenantId::parse("other-tenant")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable(
                "veoveo-artifact-service",
                "artifact-smoke",
            )?,
            &mcp_url,
            [
                "--scheme".into(),
                "datasheet".into(),
                "artifact".into(),
                artifact_id.clone().into(),
                "--output-dir".into(),
                tmpdir
                    .join("denied-cross-tenant")
                    .as_os_str()
                    .to_os_string(),
            ],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                "unknown artifact",
            ),
        )
    }?;

    let usage =
        wait_for_actual_usage_for_scheme(conformance, &mcp_url, "datasheet", &task_id, None)?;
    if usage.usage_uri != format!("datasheet://usage/task/{task_id}") {
        bail!("datasheet usage URI was wrong: {usage:?}");
    }
    let actual = usage
        .records
        .iter()
        .find(|record| record.kind == veoveo_mcp_contract::UsageKind::Actual)
        .ok_or_else(|| anyhow!("usage report had no actual record: {usage:?}"))?;
    if actual.quantity != Some(4.0)
        || actual.unit.as_deref() != Some("column")
        || actual.amount.is_some()
        || actual.currency.is_some()
    {
        bail!("datasheet usage actual record had wrong shape: {usage:?}");
    }

    let completion = run_datasheet_mcp(
        conformance,
        &mcp_url,
        [
            "complete-resource".into(),
            "--uri".into(),
            "datasheet://usage/task/{task_id}".into(),
            "--argument".into(),
            "task_id".into(),
            task_id[..8].to_string().into(),
        ],
    )?;
    contains(&completion, &task_id)?;

    let post_run_resources = run_datasheet_mcp(conformance, &mcp_url, ["resources".into()])?;
    not_contains(
        &post_run_resources,
        &format!("datasheet://usage/task/{task_id}"),
    )?;
    not_contains(&post_run_resources, &artifact_uri)?;

    let reports = run_datasheet_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "datasheet://reports".into()],
    )?;
    contains(&reports, &task_id)?;
    contains(&reports, "\"status\": \"succeeded\"")?;
    contains(&reports, "\"limit\": 100")?;
    contains(&reports, "\"nextUri\": null")?;

    let usage_catalog = run_datasheet_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "datasheet://usage".into()],
    )?;
    contains(&usage_catalog, &task_id)?;
    contains(&usage_catalog, "\"limit\": 100")?;
    contains(&usage_catalog, "\"nextCursor\": null")?;

    datasheet_child.stop_checked().await?;
    cleanup.remove_on_drop();
    println!("datasheet MCP smoke ok");
    Ok(())
}

fn run_datasheet_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let bearer = fixture_bearer(fixture_identity(veoveo_mcp_contract::ServerSlug::parse(
        "datasheet",
    )?)?)?;
    let mut all_args = vec!["--scheme".into(), "datasheet".into()];
    all_args.extend(args);
    run_direct_mcp(
        conformance,
        mcp_url,
        all_args,
        [("MCP_BEARER_TOKEN", bearer.into())],
    )
}

// Datasheet's nested Artifact value is governed by the public Artifact owner.
// Profile column values remain the Python owner's JSON at this language edge.
fn admit_profile_artifact(
    output: &Value,
    task_id: &str,
) -> Result<veoveo_artifact_contract::ArtifactMetadata> {
    let artifact: veoveo_artifact_contract::ArtifactMetadata = serde_json::from_value(
        output
            .get("artifact")
            .cloned()
            .context("profile output omitted Artifact metadata")?,
    )
    .context("profile output had invalid Artifact metadata")?;
    let scheme = veoveo_types::ResourceScheme::parse("datasheet")?;
    let expected =
        veoveo_artifact_contract::ArtifactUri::presented(&scheme, artifact.artifact_id());
    let result_uri: veoveo_artifact_contract::ArtifactUri = serde_json::from_value(
        output
            .get("resultUri")
            .cloned()
            .context("profile output omitted resultUri")?,
    )
    .context("profile output had invalid resultUri")?;
    anyhow::ensure!(
        artifact.artifact_uri == expected && result_uri == expected,
        "profile resultUri and Artifact parent must agree"
    );
    anyhow::ensure!(
        artifact.metadata.get("taskId").and_then(Value::as_str) == Some(task_id)
            && artifact.metadata.get("task_id").is_none(),
        "profile Artifact metadata must carry current taskId"
    );
    Ok(artifact)
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn current_profile_product_admits_owner_artifact_and_rejects_retired_or_mismatched_values() {
        let mut artifact: veoveo_artifact_contract::ArtifactMetadata =
            serde_json::from_str(include_str!(
                "../../../../../../platform/artifacts/contract/tests/fixtures/metadata-output.json"
            ))
            .unwrap();
        artifact = artifact
            .presented_under_scheme(&veoveo_types::ResourceScheme::parse("datasheet").unwrap());
        let task = "0197f78e-f2f0-7a6e-8a5d-f41c691e4471";
        artifact.metadata =
            serde_json::json!({"taskId": task, "artifactFormat": "datasheet_profile_json"});
        let output = serde_json::json!({"artifact": artifact, "resultUri": artifact.artifact_uri});
        assert_eq!(
            admit_profile_artifact(&output, task).unwrap().artifact_id(),
            artifact.artifact_id()
        );
        for retired in ["artifactId", "artifactUri"] {
            let mut bad = output.clone();
            let fields = bad["artifact"].as_object_mut().unwrap();
            let old = if retired == "artifactId" {
                "artifact_id"
            } else {
                "artifact_uri"
            };
            fields.insert(old.to_owned(), fields[retired].clone());
            assert!(admit_profile_artifact(&bad, task).is_err());
        }
        let mut bad = output.clone();
        bad["resultUri"] = serde_json::json!("artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471");
        assert!(admit_profile_artifact(&bad, task).is_err());
        let mut bad = output.clone();
        bad["artifact"]["metadata"]["task_id"] = serde_json::json!(task);
        assert!(admit_profile_artifact(&bad, task).is_err());
        let mut bad = output;
        bad.as_object_mut().unwrap().remove("resultUri");
        assert!(admit_profile_artifact(&bad, task).is_err());
    }
}
