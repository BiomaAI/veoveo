//! Explicit installation migration. Normal startup never writes seed definitions.
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::PathBuf};
use veoveo_mcp_contract::{TenantId, WorkContextId, agent_management as wire};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayControlStore};
use veoveo_platform_store::{
    StoreAuthLevel, WorkContextMembershipLevel, agent_management as domain,
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

#[derive(Args, Debug)]
pub(crate) struct Arguments {
    #[command(flatten)]
    store: crate::SurrealStoreArgs,
    #[arg(long)]
    control_plane: PathBuf,
    /// Reviewed seed document; never read by gateway startup.
    #[arg(long)]
    source: PathBuf,
    /// Existing principal identifier recorded as owner and migration actor.
    #[arg(long)]
    owner: String,
    /// Private file containing the exact original chat bindings, created before conversion.
    #[arg(long)]
    recovery: PathBuf,
    /// Restore original bindings before reopening the gateway after a failed cutover.
    #[arg(long)]
    restore: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Source {
    tenant: TenantId,
    work_context: WorkContextId,
    definitions: Vec<Definition>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Definition {
    id: wire::AgentDefinitionId,
    name: String,
    description: String,
    model: wire::AgentModelId,
    instructions: String,
    tools: Vec<veoveo_mcp_contract::GatewayToolName>,
    budgets: wire::Budgets,
    source_digests: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recovery {
    tenant: TenantId,
    work_context: WorkContextId,
    source_digest: String,
    bindings: Vec<domain::AgentChatImport>,
}

pub(crate) async fn run(args: Arguments) -> Result<()> {
    use sha2::{Digest, Sha256};
    ensure!(
        args.store.auth_level == StoreAuthLevel::Root,
        "agent-catalog-import requires root installation credentials and stopped gateway writers"
    );
    let bytes = std::fs::read(&args.source)?;
    ensure!(bytes.len() <= 1024 * 1024, "agent seed exceeds 1 MiB");
    let source_digest = hex::encode(Sha256::digest(&bytes));
    let source: Source = serde_json::from_slice(&bytes)?;
    ensure!(
        !source.definitions.is_empty() && source.definitions.len() <= 64,
        "seed requires one to 64 definitions"
    );
    let catalog = GatewayCatalog::load_json(&args.control_plane)?;
    let models = super::models::from_env(&catalog)?;
    let control = GatewayControlStore::connect(args.store.into_config()?).await?;
    control.migrate().await?;
    let store = control.platform_store();
    let context = store
        .artifact_read_context_version(source.tenant.as_str(), source.work_context.as_str())
        .await?
        .context("existing Work Context required")?;
    let authority = domain::AgentCatalogAuthority::new(
        deterministic_tenant_id(source.tenant.as_str())?,
        deterministic_work_context_id(source.tenant.as_str(), source.work_context.as_str())?,
        deterministic_principal_id(source.tenant.as_str(), &args.owner)?,
        context.digest,
        WorkContextMembershipLevel::Owner,
        true,
        1000,
    );
    let mut check = store.client().query("SELECT VALUE <string> id FROM workspace_run WHERE chat.tenant = $tenant AND chat.work_context = $context AND state IN ['queued', 'running'] LIMIT 1;")
        .bind(("tenant", authority.tenant.clone())).bind(("context", authority.work_context.clone())).await?.check()?;
    let active: Vec<String> = check.take(0)?;
    ensure!(
        active.is_empty(),
        "drain or cancel active chat runs and stop all gateway writers before importing"
    );
    if args.restore {
        let recovery: Recovery = serde_json::from_slice(&std::fs::read(&args.recovery)?)?;
        verify_recovery(&recovery, &source, &source_digest)?;
        let count = store
            .apply_agent_chat_import(
                &authority,
                &recovery.bindings,
                domain::AgentChatImportDirection::Restore,
            )
            .await?;
        println!(
            "Restored {count} original chat bindings; retained catalog records were not deleted."
        );
        return Ok(());
    }
    let mut mappings = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    // Validate the complete public seed before starting any catalog mutation.
    let mut admitted = Vec::new();
    for definition in &source.definitions {
        ensure!(
            keys.insert(definition.id.as_str()),
            "duplicate seed definition"
        );
        let model = models
            .iter()
            .find(|m| {
                m.id == definition.model
                    && m.tenant == source.tenant
                    && m.work_contexts.contains(&source.work_context)
            })
            .context("seed model is not admitted to this Work Context")?;
        ensure!(
            model.admits(&definition.budgets),
            "seed exceeds approved model budgets"
        );
        let content = super::projection::content(wire::Content {
            model: wire::ModelReference {
                id: model.id.clone(),
                revision: model.revision(),
            },
            instructions: definition.instructions.clone(),
            tools: definition.tools.clone(),
            budgets: definition.budgets.clone(),
            execution: wire::Execution::Chat,
        });
        content.validate()?;
        ensure!(
            definition.source_digests.len() <= 64
                && definition
                    .source_digests
                    .iter()
                    .all(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())),
            "invalid source digest"
        );
        admitted.push((definition, content));
    }
    for (definition, content) in admitted {
        let digest = content.digest()?;
        let draft = match store
            .agent_definition(&authority, definition.id.as_str())
            .await
        {
            Ok(existing) => {
                ensure!(
                    existing.draft == content
                        && existing.name == definition.name
                        && existing.description == definition.description,
                    "existing definition {} differs; import cannot overwrite authored changes",
                    definition.id
                );
                existing
            }
            Err(domain::AgentManagementError::NotFound) => {
                store
                    .mutate_agent_definition(
                        &authority,
                        definition.id.as_str(),
                        uuid::Uuid::now_v7(),
                        None,
                        domain::AgentDefinitionMutation::Create {
                            name: definition.name.clone(),
                            description: definition.description.clone(),
                            content,
                        },
                    )
                    .await?
            }
            Err(error) => return Err(error.into()),
        };
        if draft.published.is_none() {
            store
                .mutate_agent_definition(
                    &authority,
                    definition.id.as_str(),
                    uuid::Uuid::now_v7(),
                    Some(draft.revision),
                    domain::AgentDefinitionMutation::Publish {
                        digest: digest.clone(),
                        audience: vec![domain::AgentPublicationContext {
                            work_context: authority.work_context.clone(),
                            context_digest: authority.context_digest.clone(),
                        }],
                    },
                )
                .await?;
        } else {
            let current = store
                .agent_executable(&authority, definition.id.as_str(), None)
                .await?;
            ensure!(
                current.revision.digest == digest,
                "published definition differs from the seed"
            );
        }
        mappings.push(domain::AgentChatImportMapping {
            key: definition.id.to_string(),
            source_digests: definition.source_digests.clone(),
            target_digest: digest,
        });
    }
    let recovery = if args.recovery.exists() {
        let recovery: Recovery = serde_json::from_slice(&std::fs::read(&args.recovery)?)?;
        verify_recovery(&recovery, &source, &source_digest)?;
        recovery
    } else {
        let bindings = store.plan_agent_chat_import(&authority, &mappings).await?;
        let recovery = Recovery {
            tenant: source.tenant.clone(),
            work_context: source.work_context.clone(),
            source_digest: source_digest.clone(),
            bindings,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&args.recovery)?;
        file.write_all(&serde_json::to_vec_pretty(&recovery)?)?;
        file.sync_all()?;
        recovery
    };
    let count = store
        .apply_agent_chat_import(
            &authority,
            &recovery.bindings,
            domain::AgentChatImportDirection::Apply,
        )
        .await?;
    ensure!(
        store
            .plan_agent_chat_import(&authority, &mappings)
            .await?
            .is_empty(),
        "unconverted bindings remain"
    );
    println!(
        "Imported {} definitions and {count} chat bindings. Recovery details are in {}.",
        mappings.len(),
        args.recovery.display()
    );
    Ok(())
}

fn verify_recovery(recovery: &Recovery, source: &Source, digest: &str) -> Result<()> {
    ensure!(
        recovery.tenant == source.tenant
            && recovery.work_context == source.work_context
            && recovery.source_digest == digest,
        "recovery file belongs to another import"
    );
    Ok(())
}
