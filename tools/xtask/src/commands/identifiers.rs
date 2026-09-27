//! Enforce repository-owned identifier namespaces in tracked text files.
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use anyhow::{Context, Result, bail};

use crate::{context::RepositoryContext, process};

// Split retired names so the enforcement source obeys the same policy it checks.
const RETIRED: [&str; 3] = [
    concat!("io", ".veoveo"),
    concat!("veoveo", ".io"),
    concat!("ai", ".bioma"),
];

pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    let root = repository.root();
    let listing = process::output("git", ["ls-files", "-z"], Some(root))?.stdout;
    let listing = String::from_utf8(listing).context("git ls-files output is not UTF-8")?;
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in listing.split('\0').filter(|path| !path.is_empty()) {
        let file = File::open(root.join(path)).with_context(|| format!("opening {path}"))?;
        let mut reader = BufReader::new(file);
        if reader
            .fill_buf()
            .with_context(|| format!("reading {path}"))?
            .contains(&0)
        {
            continue;
        }
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading {path}"))?;
        // Search bytes even when a tracked text file is not UTF-8.
        let content = String::from_utf8_lossy(&bytes);
        checked += 1;
        for (line, identifier) in violations(Path::new(path), &content) {
            failures.push(format!(
                "{path}:{line}: retired identifier namespace {identifier}"
            ));
        }
    }
    if !failures.is_empty() {
        bail!(
            "{} retired identifier namespaces; use ai.veoveo/ for MCP metadata, \
             ai.veoveo. for OCI labels, and veoveo.ai/ for formats and Kubernetes keys:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    println!("checked identifier namespaces in {checked} tracked text files");
    Ok(())
}

fn violations<'a>(path: &Path, content: &'a str) -> Vec<(usize, &'a str)> {
    if path == Path::new("docs/PLATFORM_FOUNDATIONS_PLAN.md") {
        return Vec::new();
    }
    let mut section = "";
    let mut failures = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.starts_with("# ") || line.starts_with("## ") {
            section = line;
        }
        let allowed = (path == Path::new("AGENTS.md") && section == "## Naming")
            || (path == Path::new("docs/CONTRACT_EVOLUTION.md")
                && section == "## CE-10: Repository Identifiers Use The Veoveo Domain");
        if !allowed {
            for retired in RETIRED {
                if let Some(offset) = line.find(retired) {
                    failures.push((index + 1, &line[offset..offset + retired.len()]));
                }
            }
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_retired_names_in_code_fixtures_and_history() {
        for path in ["src/lib.rs", "tests/fixture.json", "docs/measurement.md"] {
            for retired in RETIRED {
                let content = format!("first line\n{retired}/example/v1\n");
                assert_eq!(violations(Path::new(path), &content), vec![(2, retired)]);
            }
        }
    }

    #[test]
    fn permits_only_the_named_migration_sections() {
        for (path, heading) in [
            ("AGENTS.md", "## Naming"),
            (
                "docs/CONTRACT_EVOLUTION.md",
                "## CE-10: Repository Identifiers Use The Veoveo Domain",
            ),
        ] {
            let old = RETIRED[0];
            let content = format!("{old}\n{heading}\n{old}\n### Details\n{old}\n## Next\n{old}");
            assert_eq!(
                violations(Path::new(path), &content),
                vec![(1, old), (7, old)]
            );
            assert_eq!(violations(Path::new("nested/AGENTS.md"), &content).len(), 4);
        }
        assert!(violations(Path::new("docs/PLATFORM_FOUNDATIONS_PLAN.md"), RETIRED[0]).is_empty());
    }

    #[test]
    fn accepts_current_names_and_unrelated_owned_domains() {
        let content = "ai.veoveo/task-retention-pin ai.veoveo.build.mode \
                       veoveo.ai/live-view/v4 https://veoveo.bioma.ai \
                       io.modelcontextprotocol/clientCapabilities";
        assert!(violations(Path::new("src/lib.rs"), content).is_empty());
    }
}
