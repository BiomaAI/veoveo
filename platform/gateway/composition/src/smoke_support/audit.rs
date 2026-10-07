//! Smoke assertions select fixture partitions and typed activities in SQL.
use super::*;
use serde::Serialize;
use veoveo_mcp_contract::{GatewayControlPlane, audit::*};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
use veoveo_types::{AuthMethod, AuthReasonCode, DataLabelId, OAuthClientId, PrincipalId};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SmokeAuditSelection {
    Class(AuditClass),
    Read(AuditReadMethod),
    Discovery(DiscoveryKind),
    DiscoveryDenials {
        collection: DiscoveryKind,
        denied: u32,
    },
    ToolAdmission,
    ToolCompletion,
    Task(TaskActivity),
    AuthenticationMethod(AuthMethod),
    AuthenticationReason(AuthReasonCode),
    Reason(AuditReason),
    AdminAdmission(AdministrativeOperation),
    AdminCompletion(AdministrativeOperation),
    ActorKind {
        class: AuditClass,
        kind: AuditPrincipalKind,
    },
    DelegatedActor {
        class: AuditClass,
        actor: PrincipalId,
        delegator: PrincipalId,
        client: OAuthClientId,
    },
    DataLabel {
        class: AuditClass,
        label: DataLabelId,
    },
}

impl SmokeAuditSelection {
    fn query(&self) -> &'static str {
        match self {
            Self::Class(_) => include_str!("../smoke/queries/audit/count_class.surql"),
            Self::Read(_) => include_str!("../smoke/queries/audit/count_read.surql"),
            Self::Discovery(_) => include_str!("../smoke/queries/audit/count_discovery.surql"),
            Self::DiscoveryDenials { .. } => {
                include_str!("../smoke/queries/audit/count_discovery_denials.surql")
            }
            Self::ToolAdmission => {
                include_str!("../smoke/queries/audit/count_tool_admission.surql")
            }
            Self::ToolCompletion => {
                include_str!("../smoke/queries/audit/count_tool_completion.surql")
            }
            Self::Task(_) => include_str!("../smoke/queries/audit/count_task.surql"),
            Self::AuthenticationMethod(_) => {
                include_str!("../smoke/queries/audit/count_authentication_method.surql")
            }
            Self::AuthenticationReason(_) => {
                include_str!("../smoke/queries/audit/count_authentication_reason.surql")
            }
            Self::Reason(_) => include_str!("../smoke/queries/audit/count_reason.surql"),
            Self::AdminAdmission(_) => {
                include_str!("../smoke/queries/audit/count_admin_admission.surql")
            }
            Self::AdminCompletion(_) => {
                include_str!("../smoke/queries/audit/count_admin_completion.surql")
            }
            Self::ActorKind { .. } => include_str!("../smoke/queries/audit/count_actor_kind.surql"),
            Self::DelegatedActor { .. } => {
                include_str!("../smoke/queries/audit/count_delegated_actor.surql")
            }
            Self::DataLabel { .. } => include_str!("../smoke/queries/audit/count_data_label.surql"),
        }
    }
}

pub struct SmokeAudit {
    store: PlatformStore,
    partitions: Vec<AuditPartition>,
}

impl SmokeAudit {
    pub async fn connect(platform: &PlatformStoreSmoke, control_plane: &Path) -> Result<Self> {
        let plane: GatewayControlPlane = serde_json::from_slice(&fs::read(control_plane)?)?;
        let partitions = std::iter::once(AuditPartition::Installation)
            .chain(
                plane
                    .tenants
                    .into_iter()
                    .map(|tenant| AuditPartition::Tenant(tenant.id)),
            )
            .collect();
        let store = tokio::time::timeout(
            Duration::from_secs(15),
            PlatformStore::connect(
                StoreConfig::builder(
                    &platform.endpoint,
                    &platform.namespace,
                    &platform.database,
                    StoreCredentials::database(
                        SURREAL_RUNTIME_USER,
                        secrecy::SecretString::from(SURREAL_RUNTIME_PASSWORD),
                    ),
                )
                .build()?,
            ),
        )
        .await
        .context("audit fixture connection exceeded 15 seconds")??;
        Ok(Self { store, partitions })
    }

    async fn count(
        &self,
        selection: &SmokeAuditSelection,
        outcome: Option<AuditOutcome>,
    ) -> Result<u64> {
        tokio::time::timeout(Duration::from_secs(15), async {
            let mut total = 0;
            for partition in &self.partitions {
                let mut response = self
                    .store
                    .client()
                    .query(selection.query())
                    .bind(("partition", partition.storage_key()))
                    .bind(("selection", serde_json::to_value(selection)?))
                    .bind(("outcome", outcome.map(serde_json::to_value).transpose()?))
                    .await?
                    .check()?;
                let counts: Vec<u64> = response.take(0)?;
                anyhow::ensure!(
                    counts.len() <= 1,
                    "audit GROUP ALL returned multiple counts"
                );
                total += counts.first().copied().unwrap_or_default();
            }
            Ok(total)
        })
        .await
        .context("audit fixture count exceeded 15 seconds")?
    }

    pub async fn at_least(
        &self,
        selection: SmokeAuditSelection,
        outcome: Option<AuditOutcome>,
        minimum: u64,
    ) -> Result<()> {
        let actual = self.count(&selection, outcome).await?;
        anyhow::ensure!(
            actual >= minimum,
            "{selection:?}/{outcome:?}: expected at least {minimum} records, got {actual}"
        );
        Ok(())
    }

    pub async fn exact(
        &self,
        selection: SmokeAuditSelection,
        outcome: Option<AuditOutcome>,
        expected: u64,
    ) -> Result<()> {
        let actual = self.count(&selection, outcome).await?;
        anyhow::ensure!(
            actual == expected,
            "{selection:?}/{outcome:?}: expected {expected} records, got {actual}"
        );
        Ok(())
    }

    /// Exercise the public CLI separately from fixture-only activity assertions.
    pub fn assert_cli(&self, gateway: &Path, platform: &PlatformStoreSmoke) -> Result<()> {
        let targets = veoveo_gateway_catalog::audit_target_registry()?;
        for partition in &self.partitions {
            let mut args: Vec<OsString> = vec![
                "audit".into(),
                "list".into(),
                "--limit".into(),
                "1000".into(),
            ];
            match partition {
                AuditPartition::Installation => args.push("--installation".into()),
                AuditPartition::Tenant(tenant) => {
                    args.push("--tenant".into());
                    args.push(tenant.to_string().into());
                }
            }
            let page: AuditPage =
                targets
                    .decoder()
                    .from_str(&run_checked(gateway, args, platform.runtime_env())?)?;
            anyhow::ensure!(
                page.records
                    .iter()
                    .all(|row| row.draft.partition() == partition),
                "audit list crossed its requested partition"
            );
        }
        Ok(())
    }
}
