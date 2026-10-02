//! CLI admission and report I/O; source assertions live in the shared runner.
use std::{collections::BTreeSet, io::Write, path::PathBuf};

use anyhow::{Result, ensure};
use clap::{Args, ValueEnum};
use veoveo_mcp_conformance::{
    ConformanceCredentials, KnowledgeRoute, KnowledgeSourceTarget,
    knowledge_probes::KnowledgeProbes, run_knowledge_source_conformance,
};
use veoveo_types::{ResourceScheme, ServerSlug};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum Route {
    Direct,
    Gateway,
}

#[derive(Args)]
pub(super) struct SourceChecks {
    /// Source owner declared by the knowledge collections.
    #[arg(long)]
    server: ServerSlug,
    /// Source-owned resource scheme. Repeat when a source owns several schemes.
    #[arg(long = "owned-scheme", required = true)]
    schemes: Vec<ResourceScheme>,
    /// Tool naming and collection-owner selection at the endpoint.
    #[arg(long, value_enum, default_value = "gateway")]
    route: Route,
    /// New machine-readable source report. Existing reports are never overwritten.
    #[arg(long)]
    report: PathBuf,
}

impl SourceChecks {
    pub(super) async fn run(&self, endpoint: &str, bearer: Option<String>) -> Result<()> {
        ensure!(!self.report.exists(), "source report already exists");
        let target = KnowledgeSourceTarget::new(
            endpoint.parse()?,
            self.server.clone(),
            self.schemes.iter().cloned().collect::<BTreeSet<_>>(),
            match self.route {
                Route::Direct => KnowledgeRoute::Direct,
                Route::Gateway => KnowledgeRoute::Gateway,
            },
        )?;
        let credentials = bearer
            .map(ConformanceCredentials::bearer)
            .unwrap_or_default();
        let result =
            run_knowledge_source_conformance(&target, &credentials, &KnowledgeProbes::default())
                .await?;
        if let Some(parent) = self.report.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.report)?;
        serde_json::to_writer_pretty(&mut file, &result)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        println!(
            "source {} {}: {} check(s), report {}",
            self.server,
            if result.passed() { "passed" } else { "failed" },
            result.checks.len(),
            self.report.display()
        );
        ensure!(result.passed(), "knowledge-source conformance failed");
        Ok(())
    }
}
