//! Opt-in installation cutover; never runs against installation data by default.
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "pilot_consolidation/support.rs"]
mod support;
use anyhow::{Context, Result, ensure};
use std::{fs::File, time::Duration};
use support::*;
use surrealdb::types::{RecordId, ToSql, Value};
use veoveo_bioma_acceptance::pilot_consolidation::{PilotRebinding, apply, prepare};
use veoveo_platform_store::{
    agent_management::{
        AgentDefinition, AgentDefinitionStatus, AgentExecution, AgentRevision,
        agent_definition_record, instances::*,
    },
    deterministic_tenant_id,
};

#[tokio::test]
#[ignore = "reads live pilot records and rehearses consolidation in an isolated pinned database"]
async fn rehearse_shared_definition_cutover() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(180), rehearse()).await?
}
async fn rehearse() -> Result<()> {
    let live = installed().await?;
    let records = retained_records(&live).await?;
    let db = fixture::TestDb::new().await;
    for record in records {
        let Value::Object(ref fields) = record else {
            anyhow::bail!("record object required");
        };
        let id = fields.get("id").context("record identity")?;
        db.a.client()
            .query(format!(
                "CREATE ONLY {} CONTENT {};",
                id.to_sql(),
                record.to_sql()
            ))
            .await?
            .check()?;
    }
    let tenant = deterministic_tenant_id("bioma")?.record_id();
    // Simulate the coordinated pause in the disposable copy, leaving live pilots alone.
    db.a.client().query("UPDATE managed_agent SET desired = 'paused', observed = 'paused'; UPDATE agent_definition SET disabled = true, status = 'disabled'; UPDATE agent SET lease_expires_at = NONE, lease_owner = NONE, state = 'idle', last_episode = NONE;").await?.check()?;
    let mut definition: AgentDefinition =
        db.a.client()
            .select(agent_definition_record(&tenant, "uav-1-pilot")?)
            .await?
            .context("fixture definition")?;
    let mut revision: AgentRevision =
        db.a.client()
            .select(definition.published.clone().context("publication")?)
            .await?
            .context("fixture revision")?;
    definition.id = agent_definition_record(&tenant, "uav-pilot")?;
    definition.key = "uav-pilot".into();
    definition.name = "UAV Pilot".into();
    definition.disabled = false;
    definition.status = AgentDefinitionStatus::Enabled;
    revision.id = RecordId::new(
        "agent_definition_revision",
        surrealdb::types::Uuid::from(uuid::Uuid::now_v7()),
    );
    revision.definition = definition.id.clone();
    if let AgentExecution::Managed {
        parameters,
        resource_subscriptions,
        ..
    } = &mut revision.content.execution
    {
        parameters.remove("vehicle");
        *resource_subscriptions = vec![
            "uav-sim://control-grants".into(),
            "uav-sim://mission-plans".into(),
        ];
    }
    definition.draft = revision.content.clone();
    definition.published = Some(revision.id.clone());
    db.a.client().query("CREATE ONLY $definition.id CONTENT $definition; CREATE ONLY $revision.id CONTENT $revision;")
        .bind(("definition", definition)).bind(("revision", revision)).await?.check()?;
    let plan = prepare(&db.a, "uav-pilot-rehearsal").await?;
    let unchanged = protected_records(&db.a).await?;
    // Last-row failure must roll back all earlier rows, operations and outbox events.
    db.a.client()
        .query("UPDATE ONLY $id SET name = 'concurrent edit';")
        .bind(("id", plan[3].before.id.clone()))
        .await?
        .check()?;
    ensure!(
        apply(&db.a, &plan).await.is_err(),
        "stale fourth pilot accepted"
    );
    let first: ManagedAgentInstance =
        db.a.client()
            .select(plan[0].before.id.clone())
            .await?
            .context("first pilot")?;
    ensure!(
        first == plan[0].before,
        "failed transaction changed first pilot"
    );
    db.a.client()
        .query("UPDATE ONLY $id SET name = $before.name;")
        .bind(("id", plan[3].before.id.clone()))
        .bind(("before", plan[3].before.clone()))
        .await?
        .check()?;
    db.a.client()
        .query(
            "UPDATE agent SET lease_expires_at = time::now() + 1h WHERE agent_key = 'uav-4-pilot';",
        )
        .await?
        .check()?;
    ensure!(apply(&db.a, &plan).await.is_err(), "active writer accepted");
    db.a.client()
        .query("UPDATE agent SET lease_expires_at = NONE;")
        .await?
        .check()?;
    let ids = apply(&db.a, &plan).await?;
    ensure!(
        ids.len() == 4 && apply(&db.b, &plan).await? == ids,
        "atomic replay differs"
    );
    ensure!(
        protected_records(&db.a).await? == unchanged,
        "identity, runtime or control grants changed"
    );
    for entry in &plan {
        let instance: ManagedAgentInstance =
            db.a.client()
                .select(entry.after.id.clone())
                .await?
                .context("consolidated instance")?;
        ensure!(
            instance == entry.after && instance.active_generation > 0,
            "retained memory guard lost"
        );
    }
    let mut tampered = plan.clone();
    tampered[0].after.principal = tampered[1].after.principal.clone();
    ensure!(
        apply(&db.a, &tampered).await.is_err(),
        "identity substitution accepted"
    );
    db.a.client()
        .query("UPDATE ONLY $id SET generation += 1;")
        .bind(("id", plan[0].after.id.clone()))
        .await?
        .check()?;
    ensure!(
        apply(&db.a, &plan).await.is_err(),
        "replay overwrote later generation"
    );
    eprintln!(
        "PASS: atomic four-pilot cutover, stale edit and live-writer rejection, unchanged grants/identities/memory references, idempotent replay and later-generation fencing"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "freezes and applies the reviewed cutover after pause, disable and Kubernetes drain"]
async fn consolidate_drained_pilots() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(90), consolidate()).await?
}
async fn consolidate() -> Result<()> {
    let store = installed().await?;
    let root = directory()?;
    let path = root.join("consolidation-plan.json");
    let plan: Vec<PilotRebinding> = if path.exists() {
        serde_json::from_reader(File::open(path)?)?
    } else {
        prepare(&store, &std::env::var("VEOVEO_PILOT_CONFIG_MAP")?).await?
    };
    verify_drained(&plan)?;
    private_write(
        root.join("consolidation-plan.json"),
        &serde_json::to_vec_pretty(&plan)?,
    )?;
    let before = protected_records(&store).await?;
    private_write(
        root.join("protected-records.json"),
        &serde_json::to_vec_pretty(&before)?,
    )?;
    private_write(
        root.join("retained-kubernetes.json"),
        &serde_json::to_vec_pretty(&retained_kubernetes(&plan)?)?,
    )?;
    ensure!(
        apply(&store, &plan).await?.len() == 4,
        "cutover did not affect four pilots"
    );
    ensure!(
        protected_records(&store).await? == before,
        "cutover changed protected records"
    );
    eprintln!(
        "Four pilots consolidated while paused. Principals, grants, runtime records, PVCs and signing Secrets are unchanged."
    );
    Ok(())
}

#[tokio::test]
#[ignore = "verifies resumed pilots against the frozen consolidation plan"]
async fn consolidated_pilots_are_ready() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(180), verify_ready()).await?
}
async fn verify_ready() -> Result<()> {
    let store = installed().await?;
    let root = directory()?;
    let plan: Vec<PilotRebinding> =
        serde_json::from_reader(File::open(root.join("consolidation-plan.json"))?)?;
    let retained: Vec<RetainedKubernetes> =
        serde_json::from_reader(File::open(root.join("retained-kubernetes.json"))?)?;
    ensure!(
        retained_kubernetes(&plan)? == retained,
        "retained physical volume or signing Secret replaced"
    );
    let protected: Vec<Value> =
        serde_json::from_reader(File::open(root.join("protected-records.json"))?)?;
    ensure!(
        stable_domain_records(protected) == stable_domain_records(protected_records(&store).await?),
        "principal, vehicle grants, runtime identity or memory database changed"
    );
    for entry in &plan {
        let current: ManagedAgentInstance = store
            .client()
            .select(entry.after.id.clone())
            .await?
            .context("instance")?;
        ensure!(
            current.desired == ManagedAgentDesired::Running
                && current.observed == ManagedAgentPhase::Ready
                && current.generation == current.active_generation
                && current.generation > entry.after.generation
                && current.definition == entry.after.definition
                && current.requested_revision == entry.after.requested_revision
                && current.active_revision.as_ref() == Some(&current.requested_revision)
                && current.identity == entry.before.identity
                && current.principal == entry.before.principal
                && current.public_key == entry.before.public_key
                && current.resources == entry.after.resources,
            "pilot {} has not converged or its retained identity changed",
            current.key
        );
    }
    eprintln!(
        "PASS: four Ready instances share one definition and revision; original principal, public key, Secret UID, PVC UID and physical volume retained"
    );
    Ok(())
}

fn stable_domain_records(mut records: Vec<Value>) -> Vec<Value> {
    for record in &mut records {
        if let Value::Object(fields) = record
            && fields.contains_key("agent_key")
        {
            fields.retain(|key, _| {
                [
                    "id",
                    "tenant",
                    "work_context",
                    "agent_key",
                    "memory_database",
                ]
                .contains(&key.as_str())
            });
        }
    }
    records.sort_by_key(ToSql::to_sql);
    records
}
