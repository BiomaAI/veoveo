//! Presentation is source status; coverage verification also observes environment.
use std::{collections::BTreeMap, env, ffi::OsString, fs, io::Write, path::Path};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};

use super::{
    catalog, environment, inputs,
    model::{CommandIdentity, CoverageProfile, EvidenceClass, Outcome, PROFILE_SCHEMA, Receipt},
    storage,
};
use crate::context::RepositoryContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CurrentStatus {
    Passed,
    Failed,
    Stale,
    Unqualified,
}

fn current(
    root: &Path,
    receipt: &Receipt,
    observed: &mut BTreeMap<String, String>,
) -> Result<CurrentStatus> {
    // One invocation observes an identical closure once. This cache never crosses
    // an invocation or renews runtime evidence.
    let key = serde_json::to_string(&receipt.inputs.scope)?;
    let digest = match observed.entry(key) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(inputs::snapshot(root, &receipt.inputs.scope)?.digest)
        }
    };
    if *digest != receipt.inputs.digest {
        return Ok(CurrentStatus::Stale);
    }
    if receipt.outcome != Outcome::Passed {
        return Ok(CurrentStatus::Failed);
    }
    if !receipt.environment.reusable {
        return Ok(CurrentStatus::Unqualified);
    }
    Ok(CurrentStatus::Passed)
}

fn latest(root: &Path) -> Result<BTreeMap<String, Receipt>> {
    let index = storage::read_index(root)?;
    storage::ensure_index_complete(root, &index)?;
    let mut selected = BTreeMap::new();
    for reference in &index.receipts {
        // Every indexed receipt is integrity checked, including failed history.
        let receipt = storage::read_receipt(root, reference)?;
        if index.latest.get(&receipt.check_id) == Some(&receipt.run_id) {
            selected.insert(receipt.check_id.clone(), receipt);
        }
    }
    ensure!(
        selected.len() == index.latest.len(),
        "index selection and receipt identities differ"
    );
    Ok(selected)
}

pub(crate) fn show(repository: &RepositoryContext, github_summary: bool) -> Result<()> {
    let receipts = latest(repository.root())?;
    ensure!(!receipts.is_empty(), "no completed local evidence");
    let mut markdown = String::from(
        "# Local test evidence\n\nSource status is evaluated per check. This informational report does not establish current deployment health or release coverage.\n\n| Check | Current source | Duration | Check identity |\n|---|---|---:|---|\n",
    );
    let mut passing = 0;
    let mut failing = 0;
    let mut observed = BTreeMap::new();
    for receipt in receipts.values() {
        let status = current(repository.root(), receipt, &mut observed)?;
        passing +=
            usize::from(status == CurrentStatus::Passed || status == CurrentStatus::Unqualified);
        failing += usize::from(status == CurrentStatus::Failed);
        markdown.push_str(&format!(
            "| {} | {status:?} | {:.1}s | `{}` |\n",
            receipt.name,
            receipt.duration_millis as f64 / 1000.0,
            receipt.check_id
        ));
    }
    markdown.push_str("\nStale rows retain history and require new evidence when their coverage is needed. Unqualified rows record an observed pass without reusable environment coverage. Failed attempts remain in the immutable receipt history.\n");
    print!("{markdown}");
    if github_summary && let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(markdown.as_bytes())?;
    }
    ensure!(
        failing == 0 && passing > 0,
        "local evidence has a current failure or no current passing observations"
    );
    Ok(())
}

pub(super) fn validate_coverage(
    profile: &CoverageProfile,
    receipts: &BTreeMap<String, Receipt>,
    now: DateTime<Utc>,
) -> Result<()> {
    ensure!(
        profile.schema_version == PROFILE_SCHEMA
            && !profile.name.is_empty()
            && !profile.requirements.is_empty()
            && profile.requirements.len() <= 512,
        "invalid coverage profile"
    );
    let mut identities = std::collections::BTreeSet::new();
    for requirement in &profile.requirements {
        ensure!(
            identities.insert(&requirement.check_id),
            "duplicate coverage requirement"
        );
        let receipt = receipts
            .get(&requirement.check_id)
            .context("required check has no receipt")?;
        ensure!(
            receipt.outcome == Outcome::Passed && receipt.environment.reusable,
            "required check has no reusable passing result"
        );
        ensure!(
            receipt.environment.runtime.class == requirement.class
                && requirement.class != EvidenceClass::Unclassified,
            "check does not provide required evidence class"
        );
        ensure!(
            receipt.finished_at <= now,
            "evidence observation is in the future"
        );
        ensure!(
            receipt.environment.runtime.bindings == requirement.bindings,
            "runtime configuration/artifact bindings differ"
        );
        if requirement.class != EvidenceClass::Source {
            ensure!(
                !requirement.bindings.is_empty()
                    && requirement.max_age_seconds.is_some()
                    && receipt.environment.runtime.valid_for_seconds.is_some(),
                "runtime coverage needs exact bindings and explicit freshness"
            );
        }
        for bound in [
            requirement.max_age_seconds,
            receipt.environment.runtime.valid_for_seconds,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                bound > 0 && (now - receipt.finished_at).num_seconds().unsigned_abs() <= bound,
                "runtime evidence expired"
            );
        }
    }
    Ok(())
}

pub(crate) fn verify(repository: &RepositoryContext, profile_path: &Path) -> Result<()> {
    let root = repository.root();
    let path = if profile_path.is_absolute() {
        profile_path.to_path_buf()
    } else {
        root.join(profile_path)
    };
    let canonical = fs::canonicalize(&path)?;
    canonical
        .strip_prefix(root)
        .context("coverage profile must be repository owned")?;
    ensure!(
        fs::metadata(&canonical)?.len() <= 1024 * 1024,
        "coverage profile exceeds its bound"
    );
    let profile: CoverageProfile =
        serde_json::from_slice(&fs::read(canonical)?).context("decoding coverage profile")?;
    let receipts = latest(root)?;
    validate_coverage(&profile, &receipts, Utc::now())?;
    let mut observed = BTreeMap::new();
    let mut environments = BTreeMap::new();
    for receipt in receipts.values() {
        let state = current(root, receipt, &mut observed)?;
        ensure!(
            state != CurrentStatus::Failed,
            "current known failure: {}",
            receipt.name
        );
        if profile
            .requirements
            .iter()
            .any(|requirement| requirement.check_id == receipt.check_id)
        {
            ensure!(
                state == CurrentStatus::Passed,
                "required check is stale or unqualified: {}",
                receipt.name
            );
            let CommandIdentity::Admitted { arguments } = &receipt.command else {
                anyhow::bail!("opaque command cannot provide release coverage");
            };
            let definition = catalog::lookup(
                root,
                &arguments.iter().map(OsString::from).collect::<Vec<_>>(),
            )?
            .context("required check is no longer admitted")?;
            let key = serde_json::to_string(&definition.toolchains)?;
            let current_environment = match environments.entry(key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(environment::observe(root, Some(&definition))?)
                }
            };
            ensure!(
                *current_environment == receipt.environment,
                "required execution environment changed: {}",
                receipt.name
            );
        }
    }
    println!(
        "Coverage profile {} verified ({} required checks)",
        profile.name,
        profile.requirements.len()
    );
    Ok(())
}
