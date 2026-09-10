//! Rust owns schemas and output paths; the pinned Node converter only renders types.
use crate::context::RepositoryContext;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

pub(crate) fn run(repository: &RepositoryContext, check: bool) -> Result<()> {
    let web = repository.root().join("apps/console/web");
    let schemas = [
        (
            "computers",
            serde_json::to_value(veoveo_computers_contract::schema_bundle())?,
        ),
        (
            "console",
            serde_json::to_value(veoveo_mcp_contract::console_bootstrap_schema())?,
        ),
    ];
    // Generate all artifacts before touching an output. Failed conversion leaves no partial set.
    let mut artifacts = Vec::new();
    for (name, schema) in schemas {
        let mut bytes = serde_json::to_vec_pretty(&schema)?;
        bytes.push(b'\n');
        let mut child = Command::new("node")
            .arg("tools/client-types.mjs")
            .current_dir(&web)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context(
                "starting installed client-types converter (run npm ci in apps/console/web)",
            )?;
        let write = child
            .stdin
            .take()
            .context("converter stdin")?
            .write_all(&bytes);
        let output = child
            .wait_with_output()
            .context("waiting for client-types converter")?;
        write.context("sending canonical schema")?;
        ensure!(
            output.status.success(),
            "client-types converter failed for {name}"
        );
        artifacts.push((format!("{name}.schema.json"), bytes));
        artifacts.push((format!("{name}.ts"), output.stdout));
    }
    let directory = web.join("src/generated");
    if check {
        let mut stale = Vec::new();
        for (name, bytes) in &artifacts {
            if fs::read(directory.join(name)).ok().as_deref() != Some(bytes.as_slice()) {
                stale.push(name.as_str());
            }
        }
        ensure!(
            stale.is_empty(),
            "stale client types: {}; run cargo xtask release client-types",
            stale.join(", ")
        );
    } else {
        fs::create_dir_all(&directory)?;
        for (name, bytes) in artifacts {
            fs::write(directory.join(name), bytes)?;
        }
    }
    println!(
        "Canonical Console and Computers client types {}",
        if check { "verified" } else { "generated" }
    );
    Ok(())
}
