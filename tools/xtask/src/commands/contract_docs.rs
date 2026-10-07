//! Owner discovery and deterministic projections of the checked requirement catalog.
use crate::context::RepositoryContext;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};
use veoveo_mcp_contract::docs::{
    COMPLIANCE_END, COMPLIANCE_START, ComplianceProfile, render_compliance, verify_manual,
};

pub(crate) fn run(repository: &RepositoryContext, check: bool) -> Result<()> {
    let root = repository.root();
    let mut owners = Vec::new();
    for directory in ["servers", "templates", "testing/fixtures", "showcase"] {
        discover(&root.join(directory), &mut owners)?;
    }
    owners.sort();
    owners.dedup();
    ensure!(!owners.is_empty(), "no contract owners discovered");
    reconcile_embeddings(root, &owners)?;
    for owner in &owners {
        let profile_path = owner.join("contract-compliance.json");
        let profile: ComplianceProfile = serde_json::from_slice(
            &fs::read(&profile_path)
                .with_context(|| format!("{} requires an owner profile", owner.display()))?,
        )
        .with_context(|| format!("invalid {}", profile_path.display()))?;
        let manual_path = owner.join("AGENTS.md");
        let manual = fs::read_to_string(&manual_path)
            .with_context(|| format!("read {}", manual_path.display()))?;
        let updated = replace_marked(
            &manual,
            COMPLIANCE_START,
            COMPLIANCE_END,
            &render_compliance(&profile),
        )
        .with_context(|| format!("invalid markers in {}", manual_path.display()))?;
        verify_manual(&updated, &profile)
            .with_context(|| format!("invalid manual projection in {}", manual_path.display()))?;
        emit(&manual_path, updated.as_bytes(), check)?;
        ensure!(
            owner.join("DESIGN.md").is_file(),
            "{} requires its owning design",
            owner.display()
        );
    }
    let catalog = veoveo_mcp_contract::docs::catalog::requirement_catalog();
    let mut catalog_bytes = serde_json::to_vec_pretty(&catalog)?;
    catalog_bytes.push(b'\n');
    let mut schema_bytes = serde_json::to_vec_pretty(&schemars::schema_for!(ComplianceProfile))?;
    schema_bytes.push(b'\n');
    for directory in [
        "mcp/contract/catalog",
        "sdk/python/src/veoveo_mcp/catalog",
        "servers/chart-mcp",
    ] {
        emit(
            &root.join(directory).join("requirements.json"),
            &catalog_bytes,
            check,
        )?;
        emit(
            &root.join(directory).join("compliance-profile.schema.json"),
            &schema_bytes,
            check,
        )?;
    }
    let mut coverage_bytes =
        serde_json::to_vec_pretty(&veoveo_mcp_conformance::requirements::requirement_coverage())?;
    coverage_bytes.push(b'\n');
    emit(
        &root.join("mcp/conformance/catalog/coverage.json"),
        &coverage_bytes,
        check,
    )?;
    let mut table = String::from(
        "<!-- veoveo:requirement-catalog:start -->\n| ID | Level | Requirement |\n|---|---|---|\n",
    );
    for item in catalog.requirements {
        let level = match item.level {
            veoveo_mcp_contract::docs::catalog::RequirementLevel::Must => "MUST",
            veoveo_mcp_contract::docs::catalog::RequirementLevel::MustWhenAdopted => {
                "MUST when adopted"
            }
        };
        table.push_str(&format!(
            "| {} | {level} | {} |\n",
            item.id.as_str(),
            item.text
        ));
    }
    table.push_str("<!-- veoveo:requirement-catalog:end -->");
    let design_path = root.join("mcp/contract/DESIGN.md");
    let design = fs::read_to_string(&design_path)?;
    let updated = replace_marked(
        &design,
        "<!-- veoveo:requirement-catalog:start -->",
        "<!-- veoveo:requirement-catalog:end -->",
        &table,
    )?;
    emit(&design_path, updated.as_bytes(), check)?;
    println!(
        "{} owner compliance profiles and catalog projections {}",
        owners.len(),
        if check { "verified" } else { "generated" }
    );
    Ok(())
}

fn emit(path: &Path, bytes: &[u8], check: bool) -> Result<()> {
    if check {
        ensure!(
            fs::read(path).with_context(|| format!("missing generated {}", path.display()))?
                == bytes,
            "stale generated {}; run cargo xtask release contract-docs",
            path.display()
        );
    } else {
        fs::create_dir_all(path.parent().expect("output parent"))?;
        fs::write(path, bytes)?;
    }
    Ok(())
}

fn discover(path: &Path, owners: &mut Vec<PathBuf>) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    let rust_owner = if path.join("Cargo.toml").is_file() {
        let manifest: toml::Value = toml::from_str(&fs::read_to_string(path.join("Cargo.toml"))?)?;
        manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .is_some_and(|name| name.ends_with("-mcp"))
    } else {
        false
    };
    let python_owner = if path.join("pyproject.toml").is_file() {
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(path.join("pyproject.toml"))?)?;
        manifest
            .get("tool")
            .and_then(|v| v.get("hatch"))
            .and_then(|v| v.get("build"))
            .and_then(|v| v.get("hooks"))
            .and_then(|v| v.get("custom"))
            .and_then(|v| v.get("package"))
            .is_some()
    } else {
        false
    };
    if rust_owner || python_owner || path.join("build-docs.mjs").is_file() {
        owners.push(path.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && !matches!(
                entry.file_name().to_str(),
                Some("node_modules" | "target" | ".venv" | ".git")
            )
        {
            discover(&entry.path(), owners)?;
        }
    }
    Ok(())
}

/// Reconcile actual Rust document selections with package discovery. Language build
/// hooks are selected from their manifests, not from a compiled domain registry.
fn reconcile_embeddings(root: &Path, owners: &[PathBuf]) -> Result<()> {
    let mut selections = std::collections::BTreeSet::new();
    for directory in ["servers", "templates", "testing/fixtures", "showcase"] {
        collect_embeddings(&root.join(directory), owners, &mut selections)?;
    }
    for owner in owners
        .iter()
        .filter(|owner| owner.join("Cargo.toml").is_file())
    {
        ensure!(
            selections.contains(owner),
            "{} has no actual server_docs! selection",
            owner.display()
        );
    }
    Ok(())
}

fn collect_embeddings(
    path: &Path,
    owners: &[PathBuf],
    selections: &mut std::collections::BTreeSet<PathBuf>,
) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let candidate = entry.path();
        if entry.file_type()?.is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("node_modules" | "target" | ".venv" | ".git")
            ) {
                collect_embeddings(&candidate, owners, selections)?;
            }
        } else if candidate
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            let source = fs::read_to_string(&candidate)?;
            if document_selections(&source)
                .with_context(|| format!("parse document selections in {}", candidate.display()))?
                == 0
            {
                continue;
            }
            let manifest_owner = candidate
                .ancestors()
                .find(|ancestor| ancestor.join("Cargo.toml").is_file())
                .with_context(|| {
                    format!(
                        "document selection lacks a package: {}",
                        candidate.display()
                    )
                })?;
            ensure!(
                owners.iter().any(|owner| owner == manifest_owner),
                "undiscovered document owner {}",
                manifest_owner.display()
            );
            selections.insert(manifest_owner.to_path_buf());
        }
    }
    Ok(())
}

fn document_selections(source: &str) -> syn::Result<usize> {
    struct Selections(usize);
    impl<'ast> syn::visit::Visit<'ast> for Selections {
        fn visit_macro(&mut self, node: &'ast syn::Macro) {
            if node
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "server_docs")
            {
                self.0 += 1;
            }
            syn::visit::visit_macro(self, node);
        }
    }
    let syntax = syn::parse_file(source)?;
    let mut visitor = Selections(0);
    syn::visit::Visit::visit_file(&mut visitor, &syntax);
    Ok(visitor.0)
}

fn replace_marked(source: &str, start: &str, end: &str, replacement: &str) -> Result<String> {
    ensure!(
        source.matches(start).count() == 1 && source.matches(end).count() == 1,
        "expected one generated marker pair"
    );
    let begin = source.find(start).expect("one marker");
    let end_start = source.find(end).expect("one marker");
    ensure!(begin < end_start, "reversed generated markers");
    let finish = end_start + end.len();
    Ok(format!(
        "{}{}{}",
        &source[..begin],
        replacement,
        &source[finish..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marked_projection_preserves_surrounding_bytes_and_rejects_bad_markers() {
        assert_eq!(
            replace_marked("before<S>old<E>after", "<S>", "<E>", "<S>new<E>").unwrap(),
            "before<S>new<E>after"
        );
        for source in ["<S>missing", "<E>before<S>", "<S>one<S>two<E>"] {
            assert!(replace_marked(source, "<S>", "<E>", "replacement").is_err());
        }
    }
    #[test]
    fn syntax_admits_whitespace_in_actual_document_selections() {
        for source in [
            "fn docs() { server_docs!(\"fixture\"); }",
            "fn docs() { veoveo_mcp_contract::server_docs ! (\"fixture\"); }",
        ] {
            assert_eq!(document_selections(source).unwrap(), 1);
        }
        assert_eq!(
            document_selections("// server_docs!(ignored)\nfn other() {}").unwrap(),
            0
        );
    }
}
