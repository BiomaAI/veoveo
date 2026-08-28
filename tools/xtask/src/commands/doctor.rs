use std::fs;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{commands::builder, context::RepositoryContext, process};

pub(crate) const UV_VERSION: &str = "0.11.32";

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const REVIEWED_PROJECTION_SCRATCH_MAX: u64 = 96 * MIB;
const REVIEWED_PROJECTION_CONCURRENCY_MAX: u64 = 2;
const REVIEWED_PROJECTION_DEADLINE_MS_MAX: u64 = 15_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Values {
    recording: RecordingValues,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UavValues {
    recording: UavRecordingValues,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UavRecordingValues {
    maximum_segment_bytes: u64,
    maximum_segment_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordingValues {
    capture_layer_max_bytes: u64,
    spool_minimum_free_bytes: u64,
    persistence: PersistenceValues,
    catalog_cache: CatalogCacheValues,
    projection: ProjectionValues,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistenceValues {
    size: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogCacheValues {
    size: String,
    managed_bytes: u64,
    minimum_free_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionValues {
    aggregate_scratch_bytes: u64,
    minimum_free_bytes: u64,
    concurrency: u64,
    deadline_milliseconds: u64,
}

pub(crate) fn run(repository: &RepositoryContext) -> Result<()> {
    let cargo = process::output_text("cargo", ["--version"], Some(repository.root()))?;
    let git = process::output_text("git", ["--version"], Some(repository.root()))?;
    let docker = process::output_text("docker", ["--version"], Some(repository.root()))?;
    let curl = process::output_text("curl", ["--version"], Some(repository.root()))?;
    let uv = process::output_text("uv", ["--version"], Some(repository.root()))?;
    let buildx = builder::installed_buildx_version(repository)?;
    ensure!(
        buildx == builder::BUILDX_VERSION,
        "Docker Buildx {buildx} is installed; Veoveo requires {}",
        builder::BUILDX_VERSION
    );
    ensure!(
        installed_uv_version(&uv) == Some(UV_VERSION),
        "{} is installed; Veoveo requires uv {UV_VERSION}",
        uv.trim()
    );

    println!("{}", first_line(&cargo));
    println!("{}", first_line(&git));
    println!("{}", first_line(&docker));
    println!("{}", first_line(&curl));
    println!("{}", first_line(&uv));
    println!("Docker Buildx {buildx}");
    validate_recording_catalog_contract(repository)?;
    println!("Repository tool prerequisites are present");
    Ok(())
}

fn validate_recording_catalog_contract(repository: &RepositoryContext) -> Result<()> {
    let values_path = repository.root().join("deploy/helm/veoveo/values.yaml");
    let values: Values = serde_yaml_ng::from_slice(
        &fs::read(&values_path).with_context(|| format!("reading {}", values_path.display()))?,
    )
    .with_context(|| format!("parsing {}", values_path.display()))?;
    let uav_values_path = repository
        .root()
        .join("showcase/uav-sim/deploy/helm/values.yaml");
    let uav_values: UavValues = serde_yaml_ng::from_slice(
        &fs::read(&uav_values_path)
            .with_context(|| format!("reading {}", uav_values_path.display()))?,
    )
    .with_context(|| format!("parsing {}", uav_values_path.display()))?;
    let cache_capacity = parse_binary_quantity(&values.recording.catalog_cache.size)?;
    let spool_capacity = parse_binary_quantity(&values.recording.persistence.size)?;
    let shared_cache_floor = values
        .recording
        .catalog_cache
        .minimum_free_bytes
        .max(values.recording.projection.minimum_free_bytes);
    let cache_required = values
        .recording
        .catalog_cache
        .managed_bytes
        .checked_add(values.recording.projection.aggregate_scratch_bytes)
        .and_then(|value| value.checked_add(shared_cache_floor))
        .context("recording catalog-cache budget overflow")?;
    ensure!(
        cache_required <= cache_capacity,
        "recording catalog cache needs {} bytes for managed layers, projection scratch, and free headroom but its PVC is {} bytes",
        cache_required,
        cache_capacity
    );
    ensure!(
        values.recording.projection.aggregate_scratch_bytes <= REVIEWED_PROJECTION_SCRATCH_MAX,
        "recording projection scratch exceeds the reviewed 96 MiB maximum"
    );
    ensure!(
        (1..=REVIEWED_PROJECTION_CONCURRENCY_MAX)
            .contains(&values.recording.projection.concurrency),
        "recording projection concurrency must be within 1..=2"
    );
    ensure!(
        (1..=REVIEWED_PROJECTION_DEADLINE_MS_MAX)
            .contains(&values.recording.projection.deadline_milliseconds),
        "recording projection deadline must be within 1..=15000 ms"
    );
    let spool_reservation = values
        .recording
        .capture_layer_max_bytes
        .checked_mul(4)
        .and_then(|value| value.checked_add(values.recording.spool_minimum_free_bytes))
        .context("recording spool budget overflow")?;
    ensure!(
        spool_reservation <= spool_capacity,
        "recording spool cannot hold the worst-case journal/materialization reservation plus its free-space floor"
    );
    ensure!(
        uav_values.recording.maximum_segment_bytes <= 4 * GIB,
        "UAV recording maximum segment bytes exceeds the reviewed 4 GiB ceiling"
    );
    ensure!(
        uav_values.recording.maximum_segment_seconds <= 4 * 60 * 60,
        "UAV recording maximum segment age exceeds the reviewed four-hour ceiling"
    );
    ensure!(
        uav_values
            .recording
            .maximum_segment_bytes
            .checked_mul(2)
            .is_some_and(
                |two_segments| two_segments <= values.recording.catalog_cache.managed_bytes
            ),
        "recording managed cache must hold two maximum-size UAV recording segments"
    );

    reject_obsolete_recording_surfaces(repository)?;
    println!(
        "Recording catalog budgets are coherent: {} GiB cache PVC, {} GiB spool PVC",
        cache_capacity / GIB,
        spool_capacity / GIB
    );
    println!(
        "UAV recording rotation is bounded at {} GiB or {} hours",
        uav_values.recording.maximum_segment_bytes / GIB,
        uav_values.recording.maximum_segment_seconds / (60 * 60)
    );
    println!(
        "HINT after recording schema changes, run the live SurrealDB catalog transaction test; multi-statement response indexes are wire behavior"
    );
    println!(
        "HINT before image work, run `cargo xtask release preflight --expected-growth-gib <estimate> --kubernetes-node <node>`"
    );
    println!(
        "HINT local-path PVC capacity is not a filesystem quota; compare live recording storage diagnostics with node free space and DiskPressure"
    );
    println!(
        "HINT hard-cut resets must delete large SurrealDB row sets in bounded record-ID batches; one unbounded changefeed delete can exhaust database memory"
    );
    println!(
        "HINT suspend the owning GitOps reconciler before quiescing recording Deployments; a manual scale can be drift-corrected during the reset"
    );
    println!(
        "HINT run focused host-safe UAV recording policy tests from `showcase/uav-sim/runtime` with `PYTHONPATH=. python -m unittest tests/test_recording_segments.py`; the full runtime suite uses image-only Isaac dependencies"
    );
    Ok(())
}

fn reject_obsolete_recording_surfaces(repository: &RepositoryContext) -> Result<()> {
    const CONTRACT_PATHS: [&str; 7] = [
        "servers/recording-mcp/DESIGN.md",
        "servers/recording-mcp/AGENTS.md",
        "docs/RECORDINGS.md",
        "deploy/helm/veoveo/values.yaml",
        "deploy/helm/veoveo/templates/recording.yaml",
        "configs/gateway.local.json",
        "examples/bioma/gateway.json",
    ];
    const OBSOLETE: [&str; 6] = [
        "recording-playback/v8",
        "query_recording",
        "hub-query",
        "SegmentId",
        "SegmentRecord",
        "recording-query",
    ];
    for relative in CONTRACT_PATHS {
        let path = repository.root().join(relative);
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("reading recording contract {}", path.display()))?;
        for obsolete in OBSOLETE {
            ensure!(
                !contents.contains(obsolete),
                "recording hard cut left obsolete surface `{obsolete}` in {relative}"
            );
        }
    }
    Ok(())
}

fn parse_binary_quantity(value: &str) -> Result<u64> {
    let (number, multiplier) = if let Some(number) = value.strip_suffix("Gi") {
        (number, GIB)
    } else if let Some(number) = value.strip_suffix("Mi") {
        (number, MIB)
    } else if let Some(number) = value.strip_suffix("Ki") {
        (number, 1024)
    } else {
        (value, 1)
    };
    let number = number
        .parse::<u64>()
        .with_context(|| format!("parsing Kubernetes binary quantity `{value}`"))?;
    number
        .checked_mul(multiplier)
        .with_context(|| format!("Kubernetes binary quantity `{value}` overflows u64"))
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value).trim()
}

fn installed_uv_version(value: &str) -> Option<&str> {
    let mut fields = first_line(value).split_ascii_whitespace();
    (fields.next()? == "uv").then_some(fields.next()?)
}

#[cfg(test)]
mod tests {
    use super::{GIB, MIB, UV_VERSION, installed_uv_version, parse_binary_quantity};

    #[test]
    fn uv_version_accepts_the_pinned_release_with_an_upstream_target_suffix() {
        assert_eq!(
            installed_uv_version("uv 0.11.32 (x86_64-unknown-linux-gnu)\n"),
            Some(UV_VERSION)
        );
        assert_eq!(installed_uv_version("uv 0.11.32\n"), Some(UV_VERSION));
        assert_ne!(installed_uv_version("uv 0.11.31\n"), Some(UV_VERSION));
        assert_eq!(installed_uv_version("unexpected 0.11.32\n"), None);
    }

    #[test]
    fn parses_recording_pvc_binary_quantities() {
        assert_eq!(parse_binary_quantity("10Gi").unwrap(), 10 * GIB);
        assert_eq!(parse_binary_quantity("96Mi").unwrap(), 96 * MIB);
        assert!(parse_binary_quantity("2G").is_err());
    }
}
