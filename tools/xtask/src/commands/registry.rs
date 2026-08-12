use std::{process::Command, time::Instant};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::RegistryTransport;

/// Proves that the publication host can reach the selected OCI Distribution endpoint.
pub(crate) fn preflight(address: &str, transport: RegistryTransport) -> Result<()> {
    let scheme = match transport {
        RegistryTransport::Tls => "https",
        RegistryTransport::InsecureHttp => "http",
    };
    let endpoint = format!("{scheme}://{address}/v2/");
    let started = Instant::now();
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--connect-timeout",
            "3",
            "--max-time",
            "5",
            "--request",
            "GET",
        ])
        .arg(&endpoint)
        .output()
        .with_context(|| format!("probing OCI registry push endpoint {endpoint}"))?;
    ensure!(
        output.status.success(),
        "OCI registry push endpoint {endpoint} is unreachable: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let status =
        String::from_utf8(output.stdout).context("OCI registry preflight status is not UTF-8")?;
    validate_status(status.trim(), &endpoint)?;
    println!(
        "Registry preflight: {endpoint} accepted in {:.3}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn validate_status(status: &str, endpoint: &str) -> Result<()> {
    ensure!(
        matches!(status, "200" | "401"),
        "OCI registry push endpoint {endpoint} returned HTTP {status}; expected an OCI Distribution /v2/ response"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_status;

    #[test]
    fn admits_open_and_authenticated_distribution_endpoints() {
        validate_status("200", "https://registry.example/v2/").expect("open registry");
        validate_status("401", "https://registry.example/v2/").expect("authenticated registry");
    }

    #[test]
    fn rejects_non_distribution_endpoint() {
        let error = validate_status("404", "https://registry.example/v2/")
            .expect_err("missing distribution endpoint");
        assert!(error.to_string().contains("returned HTTP 404"));
    }
}
