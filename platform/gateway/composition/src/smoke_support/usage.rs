use super::*;

pub fn wait_for_actual_usage(
    conformance: &Path,
    mcp_url: &str,
    task_id: &str,
    bearer_token: Option<&str>,
) -> Result<UsageReport> {
    wait_for_actual_usage_for_scheme(conformance, mcp_url, "media", task_id, bearer_token)
}

pub fn wait_for_actual_usage_for_scheme(
    conformance: &Path,
    mcp_url: &str,
    scheme: &str,
    task_id: &str,
    bearer_token: Option<&str>,
) -> Result<UsageReport> {
    let uris =
        veoveo_mcp_contract::ServerResourceUris::new(veoveo_types::ResourceScheme::parse(scheme)?);
    let usage_uri = uris.usage_task_uri(task_id);
    let bearer = match bearer_token {
        Some(token) => token.to_owned(),
        None => fixture_bearer(fixture_identity(veoveo_mcp_contract::ServerSlug::parse(
            scheme,
        )?)?)?,
    };
    for _ in 0..90 {
        veoveo_testing_support::lifecycle::owner::check_effect()?;
        let envs = [("MCP_BEARER_TOKEN", bearer.clone().into())];
        let output = run_raw(
            conformance,
            [
                "--url".into(),
                mcp_url.into(),
                "--scheme".into(),
                scheme.into(),
                "resource".into(),
                usage_uri.clone().into(),
            ],
            envs,
        )?;
        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)?;
            if let Ok(report) = serde_json::from_str::<UsageReport>(&stdout)
                && report
                    .records
                    .iter()
                    .any(|record| record.kind == UsageKind::Actual)
            {
                return Ok(report);
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("timed out waiting for actual usage for task `{task_id}`");
}

pub fn assert_usage_report(report: &UsageReport, scheme: &str, task_id: &str) -> Result<()> {
    if report.task_id != task_id {
        bail!(
            "usage report task id `{}` did not equal `{task_id}`",
            report.task_id
        );
    }
    let expected_uri =
        veoveo_mcp_contract::ServerResourceUris::new(veoveo_types::ResourceScheme::parse(scheme)?)
            .usage_task_uri(task_id);
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
    for expected_kind in [UsageKind::Estimate, UsageKind::Actual] {
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
