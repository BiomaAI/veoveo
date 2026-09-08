//! Read-only metadata for exact Helm releases, shared by installation and GPU checks.
use crate::process::output_checked;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct HelmReleaseMetadata {
    pub(crate) name: String,
    pub(crate) namespace: String,
    pub(crate) revision: HelmRevision,
    pub(crate) status: String,
    pub(crate) chart: String,
    pub(crate) app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(crate) enum HelmRevision {
    Number(u64),
    Text(String),
}

#[derive(Deserialize)]
struct HistoricalRelease {
    revision: u64,
    status: String,
}

/// Helm upgrades can use the deployed revision and roll back to the most recent
/// deployed or superseded revision after failure. Bind all of those inventories.
pub(crate) fn upgrade_revisions(
    context: &str,
    release: &HelmReleaseMetadata,
) -> Result<BTreeSet<u64>> {
    let latest = release.revision.value()?;
    if matches!(release.status.as_str(), "deployed" | "uninstalled") {
        return Ok(BTreeSet::from([latest]));
    }
    let bytes = output_checked(
        "helm",
        [
            "--kube-context",
            context,
            "history",
            &release.name,
            "--namespace",
            &release.namespace,
            "--max",
            &latest.to_string(),
            "--output=json",
        ],
        None,
    )?;
    select_upgrade_revisions(
        release,
        &serde_json::from_slice::<Vec<HistoricalRelease>>(&bytes)?,
    )
}

fn select_upgrade_revisions(
    release: &HelmReleaseMetadata,
    history: &[HistoricalRelease],
) -> Result<BTreeSet<u64>> {
    let latest = release.revision.value()?;
    let mut unique = BTreeSet::new();
    for entry in history {
        ensure!(
            entry.revision > 0 && entry.revision <= latest && unique.insert(entry.revision),
            "Helm history changed or contains duplicate revisions"
        );
    }
    ensure!(
        history
            .iter()
            .any(|entry| entry.revision == latest && entry.status == release.status),
        "Helm history differs from its observed release metadata"
    );
    let mut revisions = BTreeSet::from([latest]);
    if let Some(deployed) = history
        .iter()
        .filter(|entry| entry.status == "deployed")
        .map(|entry| entry.revision)
        .max()
    {
        revisions.insert(deployed);
    }
    if let Some(rollback) = history
        .iter()
        .filter(|entry| matches!(entry.status.as_str(), "deployed" | "superseded"))
        .map(|entry| entry.revision)
        .max()
    {
        revisions.insert(rollback);
    }
    Ok(revisions)
}

impl HelmRevision {
    pub(crate) fn value(&self) -> Result<u64> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(value) => value
                .parse()
                .with_context(|| format!("decoding Helm release revision {value:?}")),
        }
    }
}

pub(crate) fn release_metadata(
    context: &str,
    namespace: &str,
    release_name: &str,
) -> Result<Option<HelmReleaseMetadata>> {
    ensure!(
        !release_name.is_empty()
            && release_name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "Helm release name {release_name:?} is not a canonical lowercase DNS label"
    );
    let filter = format!("^{release_name}$");
    let output = output_checked(
        "helm",
        [
            "--kube-context",
            context,
            "list",
            "--namespace",
            namespace,
            "--filter",
            filter.as_str(),
            "--max",
            "2",
            "--output",
            "json",
        ],
        None,
    )
    .with_context(|| format!("listing Helm release {namespace}/{release_name}"))?;
    decode_release_metadata(&output, namespace, release_name)
}

pub(crate) fn decode_release_metadata(
    output: &[u8],
    namespace: &str,
    release_name: &str,
) -> Result<Option<HelmReleaseMetadata>> {
    let releases: Vec<HelmReleaseMetadata> = serde_json::from_slice(output)
        .with_context(|| format!("decoding Helm 4 release list for {namespace}/{release_name}"))?;
    ensure!(
        releases.len() <= 1,
        "Helm returned {} exact-name records for release {namespace}/{release_name}",
        releases.len()
    );
    let Some(release) = releases.into_iter().next() else {
        return Ok(None);
    };
    ensure!(
        release.name == release_name,
        "Helm release metadata names {:?}, expected {release_name:?}",
        release.name
    );
    ensure!(
        release.namespace == namespace,
        "Helm release {release_name} reports namespace {:?}, expected {namespace:?}",
        release.namespace
    );
    ensure!(
        release.revision.value()? > 0,
        "Helm release {namespace}/{release_name} reports a zero revision"
    );
    Ok(Some(release))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_release_checks_deployed_and_automatic_rollback_inventories() {
        let release: HelmReleaseMetadata = serde_json::from_str(r#"{"name":"platform","namespace":"veoveo","revision":"5","status":"failed","chart":"platform-1.0.0","app_version":""}"#).unwrap();
        let mut history: Vec<HistoricalRelease> = serde_json::from_str(r#"[{"revision":1,"status":"superseded"},{"revision":2,"status":"deployed"},{"revision":3,"status":"superseded"},{"revision":4,"status":"failed"},{"revision":5,"status":"failed"}]"#).unwrap();
        assert_eq!(
            select_upgrade_revisions(&release, &history).unwrap(),
            BTreeSet::from([2, 3, 5])
        );
        history[0].revision = 5;
        assert!(select_upgrade_revisions(&release, &history).is_err());
        history[0].revision = 1;
        history[4].status = "pending-upgrade".into();
        assert!(select_upgrade_revisions(&release, &history).is_err());
    }
}
