use super::*;

pub(crate) fn wait_for_actual_usage(
    conformance: &Path,
    mcp_url: &str,
    task_id: &str,
    bearer_token: Option<&str>,
) -> Result<SmokeUsageReport> {
    for _ in 0..90 {
        let envs = usage_envs(bearer_token);
        let output = run_raw(
            conformance,
            [
                "--url".into(),
                mcp_url.into(),
                "usage".into(),
                task_id.into(),
            ],
            envs,
        )?;
        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)?;
            if let Ok(report) = serde_json::from_str::<SmokeUsageReport>(&stdout)
                && report
                    .records
                    .iter()
                    .any(|record| record.kind == SmokeUsageKind::Actual)
            {
                return Ok(report);
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("timed out waiting for actual usage for task `{task_id}`");
}

pub(crate) fn usage_envs(bearer_token: Option<&str>) -> Vec<(&'static str, OsString)> {
    match bearer_token {
        Some(token) => vec![("MCP_BEARER_TOKEN", token.into())],
        None => vec![("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    }
}

pub(crate) fn assert_usage_report(
    report: &SmokeUsageReport,
    scheme: &str,
    task_id: &str,
) -> Result<()> {
    if report.task_id != task_id {
        bail!(
            "usage report task id `{}` did not equal `{task_id}`",
            report.task_id
        );
    }
    let expected_uri = format!("{scheme}://usage/task/{task_id}");
    if report.usage_uri != expected_uri {
        bail!(
            "usage report URI `{}` did not equal `{expected_uri}`",
            report.usage_uri
        );
    }
    if report
        .records
        .iter()
        .any(|record| record.task_id != task_id)
    {
        bail!("usage report contained a record for a different task: {report:?}");
    }
    for expected_kind in [SmokeUsageKind::Estimate, SmokeUsageKind::Actual] {
        let found = report.records.iter().any(|record| {
            record.kind == expected_kind
                && record.amount == Some(0.01)
                && record.currency.as_deref() == Some("USD")
        });
        if !found {
            bail!("usage report missing {expected_kind:?} USD 0.01 record: {report:?}");
        }
    }
    Ok(())
}
