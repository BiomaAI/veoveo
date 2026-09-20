//! Opt-in installation migration; private exports never enter the source tree.
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "pilot_cutover/ownership.rs"]
mod ownership;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
use surrealdb::types::{RecordId, RecordIdKey, ToSql, Value};
use uuid::Uuid;
use veoveo_bioma_acceptance::pilot_cutover::{PilotAdoption, adopt};
use veoveo_mcp_contract::agent_management::RuntimeTemplate;
use veoveo_mcp_gateway::{GatewayCatalog, managed_agents::runtime_template_revision};
use veoveo_platform_store::{
    AgentRecord, PlatformStore, PrincipalRecord, StoreConfig, StoreCredentials,
    WorkContextMembershipLevel,
    agent_management::{
        AgentDefinition, AgentExecution, AgentRevision, agent_definition_record, instances::*,
    },
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

fn directory() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("VEOVEO_PILOT_EXPORT_DIRECTORY")?).canonicalize()?)
}
async fn installed() -> Result<PlatformStore> {
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
fn private_write(path: PathBuf, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        ensure!(
            std::fs::read(&path)? == bytes,
            "existing private export differs"
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
#[derive(Deserialize)]
struct Keys {
    keys: Vec<PublicKey>,
}
#[derive(Deserialize)]
struct PublicKey {
    kid: String,
    n: String,
    e: String,
    kty: String,
}

async fn prepare(store: &PlatformStore) -> Result<Vec<PilotAdoption>> {
    let template: RuntimeTemplate =
        serde_json::from_reader(File::open(std::env::var("VEOVEO_PILOT_TEMPLATE_FILE")?)?)?;
    let catalog = GatewayCatalog::load_json(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gateway.json"),
    )?;
    let profile = catalog
        .profile(&template.profile)
        .context("template profile")?;
    let server = catalog
        .authorization_server(&profile.authorization_server)
        .context("authorization server")?;
    let keys: Keys = serde_json::from_reader(File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/retained-pilot-jwks.json"),
    )?)?;
    let tenant = deterministic_tenant_id("bioma")?.record_id();
    let context = deterministic_work_context_id("bioma", "operations")?.record_id();
    let mut entries = Vec::new();
    for n in 1..=4 {
        let key = format!("uav-{n}-pilot");
        let runtime: AgentRecord = store
            .client()
            .query("SELECT * FROM agent WHERE tenant = $tenant AND agent_key = $key;")
            .bind(("tenant", tenant.clone()))
            .bind(("key", key.clone()))
            .await?
            .check()?
            .take::<Vec<AgentRecord>>(0)?
            .into_iter()
            .next()
            .context("existing pilot")?;
        let principal: PrincipalRecord = store
            .client()
            .select(
                deterministic_principal_id("bioma", &format!("{}#{key}", server.issuer))?
                    .record_id(),
            )
            .await?
            .context("pilot OAuth principal")?;
        let definition: AgentDefinition = store
            .client()
            .select(agent_definition_record(&tenant, &key)?)
            .await?
            .context("published definition")?;
        let revision: AgentRevision = store
            .client()
            .select(definition.published.clone().context("publication")?)
            .await?
            .context("published revision")?;
        let AgentExecution::Managed {
            template: ref template_id,
            ref template_revision,
            ref parameters,
            ..
        } = revision.content.execution
        else {
            anyhow::bail!("managed revision required");
        };
        ensure!(
            template_id == template.id.as_str()
                && template_revision == runtime_template_revision(&template).hex(),
            "reviewed template changed"
        );
        ensure!(
            parameters.get("session")
                == Some(
                    &veoveo_platform_store::agent_management::AgentTemplateParameter::Text(
                        "uav-showcase".into()
                    )
                )
                && parameters.get("vehicle")
                    == Some(
                        &veoveo_platform_store::agent_management::AgentTemplateParameter::Text(
                            format!("uav-{n}")
                        )
                    ),
            "requested pilot assignment changed"
        );
        let id = managed_agent_record(&tenant, &key)?;
        let RecordIdKey::Uuid(uuid) = &id.key else {
            anyhow::bail!("UUID required");
        };
        let workload = format!("agent-{}", Uuid::from(*uuid).simple());
        let request = Uuid::now_v7();
        let operation_id = RecordId::new(
            "managed_agent_operation",
            surrealdb::types::Uuid::from(Uuid::new_v5(
                &request,
                format!("{}:{}", tenant.to_sql(), definition.owner.to_sql()).as_bytes(),
            )),
        );
        let key_material = keys
            .keys
            .iter()
            .find(|k| k.kid == format!("uav-{n}-pilot-2026"))
            .context("retained JWK")?;
        ensure!(key_material.kty == "RSA", "RSA identity required");
        let now = store
            .client()
            .query("RETURN [time::now()];")
            .await?
            .check()?
            .take::<Vec<surrealdb::types::Datetime>>(0)?
            .into_iter()
            .next()
            .context("database clock")?
            .into();
        let instance = ManagedAgentInstance {
            id: id.clone(),
            tenant: tenant.clone(),
            work_context: context.clone(),
            owner: definition.owner.clone(),
            deployed_by: definition.owner.clone(),
            key: key.clone(),
            name: definition.name,
            definition: definition.id,
            requested_revision: revision.id.clone(),
            active_revision: Some(revision.id),
            generation: 1,
            active_generation: 1,
            dispatch_epoch: 1,
            desired: ManagedAgentDesired::Paused,
            observed: ManagedAgentPhase::Paused,
            principal: principal.id.clone(),
            identity: ManagedAgentIdentity {
                client_id: key,
                issuer: server.issuer.to_string(),
                authorization_server: server.id.to_string(),
                profile: profile.id.to_string(),
                resource: profile.protected_resource.to_string(),
                scopes: template.scopes.iter().map(ToString::to_string).collect(),
                roles: template.roles.iter().map(ToString::to_string).collect(),
                membership: WorkContextMembershipLevel::Contributor,
            },
            resources: ManagedAgentResources {
                namespace: template.workload.namespace.clone(),
                workload: workload.clone(),
                credential_secret: format!("{workload}-key"),
                volume_claim: format!("{workload}-memory"),
                template_config_map: template.workload.config_map.clone(),
                image: template.workload.image.clone(),
                storage_gib: template.workload.storage_gib,
            },
            public_key: Some(ManagedAgentPublicKey {
                kid: key_material.kid.clone(),
                n: key_material.n.clone(),
                e: key_material.e.clone(),
            }),
            operation: operation_id.clone(),
            created_at: now,
            updated_at: now,
        };
        let operation = ManagedAgentOperation {
            id: operation_id,
            instance: id,
            tenant: tenant.clone(),
            work_context: context.clone(),
            actor: definition.owner,
            request_id: request,
            fingerprint: format!("bioma-pilot-adoption/v1:{}", instance.key),
            generation: 1,
            phase: ManagedAgentPhase::Paused,
            message: Some(
                "Retained pilot adopted paused; ownership transfer precedes resume.".into(),
            ),
            lease_owner: None,
            lease_fence: 0,
            lease_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        entries.push(PilotAdoption {
            runtime,
            principal,
            instance,
            operation,
        });
    }
    veoveo_bioma_acceptance::pilot_cutover::validate(&entries)?;
    Ok(entries)
}

async fn records(store: &PlatformStore, entries: &[PilotAdoption]) -> Result<Vec<Value>> {
    let mut ids = Vec::new();
    for e in entries {
        ids.extend([
            e.runtime.id.clone(),
            e.principal.id.clone(),
            e.instance.definition.clone(),
            e.instance.requested_revision.clone(),
            e.instance.tenant.clone(),
            e.instance.work_context.clone(),
            e.instance.owner.clone(),
            deterministic_principal_id("bioma", &format!("agent:{}", e.instance.key))?.record_id(),
        ]);
    }
    ids.sort();
    ids.dedup();
    let mut result: Vec<Value> = store
        .client()
        .query("SELECT * FROM $records;")
        .bind(("records", ids.clone()))
        .await?
        .check()?
        .take(0)?;
    let present: Vec<_> = result
        .iter()
        .filter_map(|record| match record {
            Value::Object(fields) => fields.get("id").cloned(),
            _ => None,
        })
        .collect();
    let missing: Vec<_> = ids
        .iter()
        .filter(|id| !present.contains(&Value::RecordId((*id).clone())))
        .map(ToSql::to_sql)
        .collect();
    ensure!(
        missing.is_empty(),
        "targeted export is missing referenced records: {missing:?}"
    );
    let keys: Vec<_> = entries
        .iter()
        .map(|e| {
            format!(
                "{}#{}",
                e.instance.identity.issuer, e.instance.identity.client_id
            )
        })
        .collect();
    let grants: Vec<Value> = store.client().query("SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $context AND principal_key IN $keys;")
        .bind(("tenant", entries[0].instance.tenant.clone())).bind(("context", entries[0].instance.work_context.clone())).bind(("keys", keys)).await?.check()?.take(0)?;
    ensure!(
        grants.len() == 4,
        "expected the four retained vehicle grants"
    );
    result.extend(grants);
    result.sort_by_key(ToSql::to_sql);
    Ok(result)
}

#[tokio::test]
#[ignore = "reads the Bioma installation, writes private exports, and rehearses in an isolated database"]
async fn prepare_and_rehearse_retained_pilot_adoption() -> Result<()> {
    let root = directory()?;
    let live = installed().await?;
    let plan = root.join("adoption-plan.json");
    let entries = if plan.exists() {
        serde_json::from_reader(File::open(plan)?)?
    } else {
        prepare(&live).await?
    };
    let before = records(&live, &entries).await?;
    let mut sql = String::from("BEGIN TRANSACTION;\n");
    for record in &before {
        // SQL values are an opaque export boundary; lifecycle decisions use typed records.
        let Value::Object(fields) = record else {
            anyhow::bail!("record object required");
        };
        let id = fields.get("id").context("record id")?;
        sql.push_str(&format!(
            "CREATE ONLY {} CONTENT {};\n",
            id.to_sql(),
            record.to_sql()
        ));
    }
    sql.push_str("COMMIT TRANSACTION;\n");
    let isolated = fixture::TestDb::new().await;
    isolated.a.client().query(sql.clone()).await?.check()?;
    ensure!(
        records(&isolated.a, &entries).await? == before,
        "targeted record restore differs"
    );
    // Failure on the last pilot must roll back the first three and their quota.
    let mut stale = entries.clone();
    stale[3].runtime.revision += 1;
    ensure!(
        adopt(&isolated.a, &stale).await.is_err(),
        "stale source was adopted"
    );
    let count: Vec<i64> = isolated.a.client().query("RETURN [array::len((SELECT id FROM managed_agent)), array::len((SELECT id FROM managed_agent_capacity))];").await?.check()?.take(0)?;
    ensure!(
        count == [0, 0],
        "failed adoption left partial lifecycle or quota records"
    );
    let mut leased = entries.clone();
    isolated
        .a
        .client()
        .query("UPDATE ONLY $id SET lease_expires_at = time::now() + 1h;")
        .bind(("id", entries[3].runtime.id.clone()))
        .await?
        .check()?;
    leased[3].runtime = isolated
        .a
        .client()
        .select(entries[3].runtime.id.clone())
        .await?
        .context("leased fixture pilot")?;
    ensure!(
        adopt(&isolated.a, &leased).await.is_err(),
        "live writer was adopted"
    );
    isolated
        .a
        .client()
        .query("UPDATE ONLY $id CONTENT $runtime;")
        .bind(("id", entries[3].runtime.id.clone()))
        .bind(("runtime", entries[3].runtime.clone()))
        .await?
        .check()?;
    let adopted = adopt(&isolated.a, &entries).await?;
    ensure!(adopted.len() == 4, "four pilots must be adopted atomically");
    ensure!(
        adopt(&isolated.b, &entries).await? == adopted,
        "migration retry differs"
    );
    ensure!(
        records(&isolated.a, &entries).await? == before,
        "adoption changed retained records"
    );
    // The same plan cannot take over a later managed generation.
    isolated
        .a
        .client()
        .query("UPDATE ONLY $id SET generation = 2;")
        .bind(("id", entries[0].instance.id.clone()))
        .await?
        .check()?;
    ensure!(
        adopt(&isolated.b, &entries).await.is_err(),
        "adoption overwrote a later generation"
    );
    private_write(root.join("records-restore.surql"), sql.as_bytes())?;
    private_write(
        root.join("adoption-plan.json"),
        &serde_json::to_vec_pretty(&entries)?,
    )?;
    eprintln!(
        "Restored {} targeted records and rehearsed four identity-preserving, retry-safe adoptions; live records unchanged",
        before.len()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "applies the reviewed private plan only after retained PVC and Secret ownership transfer"]
async fn apply_reviewed_retained_pilot_adoption() -> Result<()> {
    let entries: Vec<PilotAdoption> =
        serde_json::from_reader(File::open(directory()?.join("adoption-plan.json"))?)?;
    ownership::verify(&entries, &directory()?)?;
    let live = installed().await?;
    let before = records(&live, &entries).await?;
    let adopted = adopt(&live, &entries).await?;
    ensure!(adopted.len() == 4, "four pilots must be adopted atomically");
    ensure!(
        records(&live, &entries).await? == before,
        "adoption changed retained records"
    );
    eprintln!(
        "Four retained pilots adopted paused; runtime identities, principals and vehicle grants unchanged"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "verifies the four resumed Bioma pilots against their private frozen adoption plan"]
async fn resumed_pilots_retain_runtime_identity_and_memory() -> Result<()> {
    let root = directory()?;
    let entries: Vec<PilotAdoption> =
        serde_json::from_reader(File::open(root.join("adoption-plan.json"))?)?;
    ownership::verify(&entries, &root)?;
    let store = installed().await?;
    for entry in entries {
        let current: ManagedAgentInstance = store
            .client()
            .select(entry.instance.id.clone())
            .await?
            .context("adopted instance missing")?;
        let runtime: AgentRecord = store
            .client()
            .select(entry.runtime.id.clone())
            .await?
            .context("retained runtime missing")?;
        let principal: PrincipalRecord = store
            .client()
            .select(entry.principal.id.clone())
            .await?
            .context("retained principal missing")?;
        ensure!(
            current.desired == ManagedAgentDesired::Running
                && current.observed == ManagedAgentPhase::Ready
                && current.generation >= 2
                && current.active_generation == current.generation,
            "pilot {} has not converged",
            current.key
        );
        ensure!(
            current.principal == entry.instance.principal
                && current.identity == entry.instance.identity
                && current.resources == entry.instance.resources
                && current.public_key == entry.instance.public_key
                && runtime.agent_key == entry.runtime.agent_key
                && runtime.tenant == entry.runtime.tenant
                && runtime.work_context == entry.runtime.work_context
                && runtime.memory_database == entry.runtime.memory_database
                && principal.enabled
                && principal.issuer == entry.principal.issuer
                && principal.subject == entry.principal.subject,
            "retained identity or memory changed for {}",
            current.key
        );
        ensure!(
            runtime
                .managed_ready
                .as_ref()
                .is_some_and(|ready| ready.generation == current.generation),
            "retained runtime did not acknowledge the managed generation"
        );
    }
    eprintln!(
        "Four resumed pilots are Ready with the original runtime IDs, principals, signing keys and physical memory volumes"
    );
    Ok(())
}
