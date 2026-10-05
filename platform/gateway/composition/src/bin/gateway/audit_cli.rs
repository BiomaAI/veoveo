//! Installation operators explicitly select one partition for every audit command.
use super::SurrealStoreArgs;
use anyhow::Context;
use base64::Engine;
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use std::{io::Write, path::PathBuf};
use veoveo_audit::{
    integrity::{AuditKeyRing, AuditVerifier},
    *,
};
use veoveo_platform_store::PlatformStore;
use veoveo_types::{PrincipalId, TenantId};
#[path = "audit_cli/keygen.rs"]
mod keygen;

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(super) struct PartitionArgs {
    #[arg(long)]
    tenant: Option<TenantId>,
    /// Select installation records using this command's administrative DB credentials.
    #[arg(long)]
    installation: bool,
}
impl PartitionArgs {
    fn partition(&self) -> AuditPartition {
        self.tenant
            .clone()
            .map_or(AuditPartition::Installation, AuditPartition::Tenant)
    }
    fn scope(&self) -> AuditReadScope {
        AuditReadScope::new(self.tenant.clone(), self.installation)
    }
}
#[derive(Debug, Args)]
pub(super) struct ReadArgs {
    #[command(flatten)]
    store: SurrealStoreArgs,
    #[command(flatten)]
    partition: PartitionArgs,
    #[arg(long)]
    after: Option<AuditRecordId>,
    #[arg(long)]
    from: Option<DateTime<Utc>>,
    #[arg(long)]
    until: Option<DateTime<Utc>>,
    #[arg(long)]
    actor: Option<PrincipalId>,
    #[arg(long)]
    trace: Option<AuditTraceId>,
    #[arg(long, value_parser=parse_class)]
    class: Option<AuditClass>,
    #[arg(long, value_parser=parse_outcome)]
    outcome: Option<AuditOutcome>,
    /// One typed AuditTarget JSON object.
    #[arg(long, value_parser=parse_target)]
    target: Option<String>,
    #[arg(long, default_value="100", value_parser=clap::value_parser!(u16).range(1..=1000))]
    limit: u16,
}
fn parse_class(input: &str) -> Result<AuditClass, String> {
    serde_json::from_value(serde_json::Value::String(input.to_owned()))
        .map_err(|error| error.to_string())
}
fn parse_outcome(input: &str) -> Result<AuditOutcome, String> {
    serde_json::from_value(serde_json::Value::String(input.to_owned()))
        .map_err(|error| error.to_string())
}
fn parse_target(input: &str) -> Result<String, String> {
    if input.len() > 16_384 {
        return Err("audit target exceeds 16 KiB".into());
    }
    // Admission uses the exact registry shared with the Store connection.
    Ok(input.to_owned())
}
#[derive(Debug, Subcommand)]
pub(super) enum AuditCommand {
    /// Create a dedicated audit seed file with mode 0600; print only its public key.
    Keygen {
        #[arg(long)]
        secret_out: PathBuf,
    },
    /// One partition-scoped page, with a typed continuation cursor.
    List(ReadArgs),
    /// JSON Lines for sealed records through a fixed checkpoint, filtered in SQL.
    Export(ReadArgs),
    /// Retained daily counts, grouped by class and outcome inside SurrealDB.
    Summary {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        partition: PartitionArgs,
        /// First UTC day included in the summary.
        #[arg(long)]
        from: chrono::NaiveDate,
        /// First UTC day excluded from the summary.
        #[arg(long)]
        until: chrono::NaiveDate,
    },
    /// Verify record hashes, signatures and chain continuity through a known tail.
    Verify {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        partition: PartitionArgs,
        /// Trusted Ed25519 public key, standard base64 of 32 bytes; repeat for rotation.
        #[arg(long, required = true)]
        public_key: Vec<String>,
        /// Trusted checkpoint immediately before the retained verification range.
        #[arg(long)]
        anchor: Option<PathBuf>,
        /// Exported checkpoint for detecting a rollback of both the DB chain and head.
        #[arg(long)]
        expected: Option<PathBuf>,
        #[arg(long, default_value = "300")]
        clock_skew_seconds: u32,
    },
}
async fn connect(
    store: SurrealStoreArgs,
    partition: &PartitionArgs,
    method: AuditReadMethod,
) -> anyhow::Result<PlatformStore> {
    let principal = PrincipalId::parse(store.username.clone())?;
    let store = PlatformStore::connect(store.into_config()?).await?;
    // The DB-authenticated operator is installation-scoped, including when the
    // selected read partition is a tenant. No CLI argument claims a tenant actor.
    let draft = AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::AuditLog {
            partition: partition.partition(),
        },
        AuditDetail::Read { method },
        AuditOutcome::Allowed,
        AuditReason::Accepted,
    )
    .actor(AuditActor {
        principal,
        kind: AuditPrincipalKind::Service,
        tenant: None,
        oauth_client: None,
        session_family: None,
        delegating_principal: None,
        managed_agent: None,
    })
    .build()?;
    let marker = draft.id();
    store.append_audit_records(&[draft]).await?;
    if method == AuditReadMethod::AuditExport {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            store.audit_wait_sealed(
                &AuditReadScope::new(None, true),
                &AuditPartition::Installation,
                marker,
            ),
        )
        .await
        .context("audit export marker is not sealed; check the gateway sealer")??;
    }
    Ok(store)
}
fn checkpoint(path: PathBuf) -> anyhow::Result<AuditCheckpoint> {
    serde_json::from_reader(std::fs::File::open(path)?).context("invalid audit checkpoint")
}
impl AuditCommand {
    pub(super) async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Keygen { secret_out } => keygen::generate(&secret_out),
            Self::List(args) => read(args, false).await,
            Self::Export(args) => read(args, true).await,
            Self::Summary {
                store,
                partition,
                from,
                until,
            } => {
                let store = connect(store, &partition, AuditReadMethod::AuditView).await?;
                let mut query = AuditDailyQuery {
                    partition: partition.partition(),
                    from: from
                        .and_hms_opt(0, 0, 0)
                        .context("invalid first day")?
                        .and_utc(),
                    until: until
                        .and_hms_opt(0, 0, 0)
                        .context("invalid last day")?
                        .and_utc(),
                    cursor: None,
                    limit: 1000,
                };
                loop {
                    let page = store.audit_daily(&partition.scope(), &query).await?;
                    for count in page.counts {
                        println!("{}", serde_json::to_string(&count)?);
                    }
                    let Some(next) = page.next else {
                        break;
                    };
                    query.cursor = Some(next);
                }
                Ok(())
            }
            Self::Verify {
                store,
                partition,
                public_key,
                anchor,
                expected,
                clock_skew_seconds,
            } => {
                let store = connect(store, &partition, AuditReadMethod::AuditView).await?;
                let selected = partition.partition();
                let scope = partition.scope();
                let mut keys = AuditKeyRing::default();
                for key in public_key {
                    let raw = base64::engine::general_purpose::STANDARD
                        .decode(key)
                        .context("invalid audit public key base64")?;
                    let raw: [u8; 32] = raw
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("audit public key must be 32 bytes"))?;
                    keys.insert(raw)?;
                }
                let anchor = anchor.map(checkpoint).transpose()?;
                let expected = match expected {
                    Some(path) => checkpoint(path)?,
                    None => store
                        .audit_partition_checkpoint(&selected)
                        .await?
                        .context("partition has no sealed checkpoint")?,
                };
                anyhow::ensure!(
                    expected.partition == selected,
                    "expected checkpoint belongs to a different partition"
                );
                let mut after = anchor.as_ref().map(|a| a.sequence);
                let mut verifier = AuditVerifier::new(
                    &keys,
                    selected.clone(),
                    anchor,
                    chrono::TimeDelta::seconds(i64::from(clock_skew_seconds)),
                )?;
                let mut blocks = 0u64;
                let mut records = 0u64;
                let mut findings = 0u64;
                while after != Some(expected.sequence) {
                    let page = store.audit_blocks(&scope, &selected, after, 16).await?;
                    anyhow::ensure!(
                        !page.is_empty(),
                        "audit chain ended before the expected checkpoint"
                    );
                    let before = after;
                    for block in page {
                        if block.head.sequence > expected.sequence {
                            break;
                        }
                        let members = store.audit_block_records(&scope, &block).await?;
                        for finding in verifier.verify(&block, &members)? {
                            eprintln!("{finding:?}");
                            findings += 1;
                        }
                        records += members.len() as u64;
                        blocks += 1;
                        after = Some(block.head.sequence);
                    }
                    anyhow::ensure!(
                        after != before,
                        "audit chain skipped the expected checkpoint"
                    );
                    anyhow::ensure!(
                        after.is_some_and(|a| a <= expected.sequence),
                        "invalid audit verification range"
                    );
                }
                verifier.finish(&expected)?;
                #[derive(serde::Serialize)]
                struct Report {
                    checkpoint: AuditCheckpoint,
                    blocks: u64,
                    records: u64,
                    clock_findings: u64,
                }
                println!(
                    "{}",
                    serde_json::to_string(&Report {
                        checkpoint: expected,
                        blocks,
                        records,
                        clock_findings: findings
                    })?
                );
                anyhow::ensure!(
                    findings == 0,
                    "audit clock-skew findings require investigation"
                );
                Ok(())
            }
        }
    }
}
async fn read(args: ReadArgs, export: bool) -> anyhow::Result<()> {
    let method = if export {
        AuditReadMethod::AuditExport
    } else {
        AuditReadMethod::AuditView
    };
    let store = connect(args.store, &args.partition, method).await?;
    let selected = args.partition.partition();
    let scope = args.partition.scope();
    let mut query = AuditQuery::new(selected.clone());
    query.cursor = args.after.map(|last_id| AuditCursor {
        order: query.order,
        partition: selected,
        last_id,
    });
    query.from = args.from;
    query.until = args.until;
    query.class = args.class;
    query.outcome = args.outcome;
    query.target = args
        .target
        .as_deref()
        .map(|input| store.audit_targets().decoder().from_str(input))
        .transpose()?;
    query.actor = args.actor;
    query.trace = args.trace;
    query.limit = args.limit;
    if !export {
        let page = store.audit_page(&scope, &query).await?;
        println!("{}", serde_json::to_string(&page)?);
        return Ok(());
    }
    query.cursor = None;
    let range = store.audit_export_range(&scope, &query.partition).await?;
    write_export_line(&AuditExportLine::Header {
        query: Box::new(query.clone()),
        first: range.first,
        checkpoint: range.checkpoint.clone(),
    })?;
    let mut count = 0u64;
    if let (Some(tail), Some(first)) = (&range.checkpoint, range.first) {
        let mut after = None;
        let mut expected = first.get();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
        while after != Some(tail.sequence) {
            let blocks = tokio::time::timeout_at(
                deadline,
                store.audit_blocks(&scope, &query.partition, after, 16),
            )
            .await??;
            anyhow::ensure!(
                !blocks.is_empty(),
                "sealed export range changed during the read"
            );
            for block in blocks {
                anyhow::ensure!(
                    block.head.sequence.get() == expected && block.head.sequence <= tail.sequence,
                    "sealed export range changed during retention"
                );
                expected += 1;
                let records = tokio::time::timeout_at(
                    deadline,
                    store.audit_filtered_block_records(&scope, &block, &query),
                )
                .await??;
                for record in records {
                    count = count
                        .checked_add(1)
                        .context("audit export count exhausted")?;
                    write_export_line(&AuditExportLine::Record {
                        record: Box::new(record),
                    })?;
                }
                after = Some(block.head.sequence);
                if after == Some(tail.sequence) {
                    break;
                }
            }
        }
    }
    write_export_line(&AuditExportLine::Complete {
        records: count,
        checkpoint: range.checkpoint,
    })
}
fn write_export_line(line: &AuditExportLine) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, line)?;
    writeln!(output)?;
    Ok(())
}
