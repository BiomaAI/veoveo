use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf, process::Command,
};
use surrealdb::types::{ToSql, Value};
use veoveo_bioma_acceptance::pilot_consolidation::PilotRebinding;
use veoveo_platform_store::{
    PlatformStore, StoreConfig, StoreCredentials, deterministic_tenant_id,
};

pub fn directory() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("VEOVEO_PILOT_EXPORT_DIRECTORY")?).canonicalize()?)
}
pub async fn installed() -> Result<PlatformStore> {
    Ok(PlatformStore::connect(
        StoreConfig::builder(
            std::env::var("VEOVEO_PILOT_STORE_ENDPOINT")?,
            "veoveo",
            "platform",
            StoreCredentials::root(
                std::env::var("VEOVEO_PILOT_STORE_USERNAME")?,
                std::env::var("VEOVEO_PILOT_STORE_PASSWORD")?,
            ),
        )
        .build()?,
    )
    .await?)
}
pub fn private_write(path: PathBuf, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        ensure!(
            std::fs::read(path)? == bytes,
            "private before-image changed"
        );
        return Ok(());
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}
pub async fn retained_records(store: &PlatformStore) -> Result<Vec<Value>> {
    let mut result = store.client().query(r#"
        LET $instances = SELECT * FROM managed_agent WHERE tenant = $tenant AND key IN $keys;
        LET $ids = array::distinct(array::flatten($instances.map(|$i| [$i.id, $i.operation, $i.definition,
            $i.requested_revision, $i.principal, $i.owner, $i.tenant, $i.work_context])));
        SELECT * FROM $ids;
        SELECT * FROM agent WHERE tenant = $tenant AND agent_key IN $keys;
        SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND principal_key IN $principals;
        SELECT * FROM agent_definition WHERE tenant = $tenant AND key IN $keys;
        SELECT * FROM agent_definition_revision WHERE definition.tenant = $tenant AND definition.key IN $keys;
    "#).bind(("tenant", deterministic_tenant_id("bioma")?.record_id()))
        .bind(("keys", keys())).bind(("principals", principals())).await?.check()?;
    let mut records: Vec<Value> = result.take(2)?;
    records.extend(result.take::<Vec<Value>>(3)?);
    records.extend(result.take::<Vec<Value>>(4)?);
    records.extend(result.take::<Vec<Value>>(5)?);
    records.extend(result.take::<Vec<Value>>(6)?);
    records.sort_by_key(ToSql::to_sql);
    records.dedup();
    ensure!(records.len() >= 28, "incomplete pilot snapshot");
    Ok(records)
}
pub async fn protected_records(store: &PlatformStore) -> Result<Vec<Value>> {
    let mut result = store.client().query(r#"
        SELECT * FROM principal WHERE id IN (SELECT VALUE principal FROM managed_agent WHERE tenant = $tenant AND key IN $keys);
        SELECT * FROM agent WHERE tenant = $tenant AND agent_key IN $keys;
        SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND principal_key IN $principals;
    "#).bind(("tenant", deterministic_tenant_id("bioma")?.record_id()))
        .bind(("keys", keys())).bind(("principals", principals())).await?.check()?;
    let mut records: Vec<Value> = result.take(0)?;
    records.extend(result.take::<Vec<Value>>(1)?);
    records.extend(result.take::<Vec<Value>>(2)?);
    ensure!(
        records.len() == 12,
        "expected four principals, runtimes and vehicle grants"
    );
    records.sort_by_key(ToSql::to_sql);
    Ok(records)
}
fn keys() -> Vec<String> {
    (1..=4).map(|n| format!("uav-{n}-pilot")).collect()
}
fn principals() -> Vec<String> {
    keys()
        .iter()
        .map(|key| format!("https://veoveo.bioma.ai/oauth#{key}"))
        .collect()
}

#[derive(Deserialize)]
struct List<T> {
    items: Vec<T>,
}
#[derive(Deserialize)]
struct Metadata {
    name: String,
    uid: String,
}
#[derive(Deserialize)]
struct Object {
    metadata: Metadata,
}
#[derive(Deserialize)]
struct Claim {
    metadata: Metadata,
    spec: ClaimSpec,
    status: Phase,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimSpec {
    volume_name: String,
}
#[derive(Deserialize)]
struct Phase {
    phase: String,
}
#[derive(Deserialize)]
struct Pod {
    spec: PodSpec,
}
#[derive(Deserialize)]
struct PodSpec {
    #[serde(default)]
    volumes: Vec<Volume>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    persistent_volume_claim: Option<PodClaim>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodClaim {
    claim_name: String,
}
#[derive(Serialize, Deserialize, PartialEq)]
pub struct RetainedKubernetes {
    key: String,
    secret_uid: String,
    claim_uid: String,
    volume: String,
}
fn get<T: serde::de::DeserializeOwned>(kind: &str, name: &str) -> Result<T> {
    let mut command = Command::new("kubectl");
    command.args(["--request-timeout=15s", "-n", "veoveo-agents", "get", kind]);
    if !name.is_empty() {
        command.arg(name);
    }
    let output = command.args(["-o", "json"]).output()?;
    ensure!(output.status.success(), "cannot inspect {kind}/{name}");
    Ok(serde_json::from_slice(&output.stdout)?)
}
pub fn verify_drained(plan: &[PilotRebinding]) -> Result<()> {
    let deployments: List<Object> = get("deployments", "")?;
    let pods: List<Pod> = get("pods", "")?;
    for entry in plan {
        let resources = &entry.before.resources;
        ensure!(
            resources.namespace == "veoveo-agents",
            "unexpected workload namespace"
        );
        ensure!(
            !deployments
                .items
                .iter()
                .any(|d| d.metadata.name == resources.workload)
                && !pods.items.iter().flat_map(|p| &p.spec.volumes).any(|v| v
                    .persistent_volume_claim
                    .as_ref()
                    .is_some_and(|c| c.claim_name == resources.volume_claim)),
            "pilot workload or memory writer still exists"
        );
    }
    Ok(())
}
pub fn retained_kubernetes(plan: &[PilotRebinding]) -> Result<Vec<RetainedKubernetes>> {
    plan.iter()
        .map(|entry| {
            let r = &entry.before.resources;
            let claim: Claim = get("pvc", &r.volume_claim)?;
            let secret: Object = get("secret", &r.credential_secret)?;
            ensure!(
                claim.status.phase == "Bound",
                "retained memory claim is not bound"
            );
            ensure!(
                !secret.metadata.uid.is_empty(),
                "signing Secret UID missing"
            );
            Ok(RetainedKubernetes {
                key: entry.before.key.clone(),
                secret_uid: secret.metadata.uid,
                claim_uid: claim.metadata.uid,
                volume: claim.spec.volume_name,
            })
        })
        .collect::<Result<Vec<_>>>()
        .context("retained Kubernetes resources")
}
