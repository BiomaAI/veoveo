//! Smoke assertions select fixture partitions and typed activities in SQL.
use super::*;
use serde::Serialize;
use veoveo_mcp_contract::{GatewayControlPlane, audit::*};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
use veoveo_types::{AuthMethod, AuthReasonCode, DataLabelId, OAuthClientId, PrincipalId};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum SmokeAuditSelection {
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
    fn predicate(&self) -> &'static str {
        match self {
            Self::Class(_) => "class = $selection.value",
            Self::Read(_) => {
                "draft.detail.kind = 'read' AND draft.detail.method = $selection.value"
            }
            Self::Discovery(_) => {
                "draft.detail.kind = 'discovery' AND draft.detail.collection = $selection.value"
            }
            Self::DiscoveryDenials { .. } => {
                "draft.detail.kind = 'discovery' AND draft.detail.collection = $selection.value.collection AND draft.detail.denied = $selection.value.denied"
            }
            Self::ToolAdmission => "draft.detail.kind = 'tool_admission'",
            Self::ToolCompletion => "draft.detail.kind = 'tool_completion'",
            Self::Task(_) => {
                "draft.detail.kind = 'task' AND draft.detail.activity = $selection.value"
            }
            Self::AuthenticationMethod(_) => {
                "draft.detail.kind = 'authentication' AND draft.detail.method = $selection.value"
            }
            Self::AuthenticationReason(_) => {
                "draft.detail.kind = 'authentication' AND draft.detail.reason = $selection.value"
            }
            Self::Reason(_) => "draft.reason = $selection.value",
            Self::AdminAdmission(_) => {
                "draft.detail.kind = 'admin_admission' AND draft.detail.operation = $selection.value"
            }
            Self::AdminCompletion(_) => {
                "draft.detail.kind = 'admin_completion' AND draft.detail.operation = $selection.value"
            }
            Self::ActorKind { .. } => {
                "class = $selection.value.class AND draft.actor.kind = $selection.value.kind"
            }
            Self::DelegatedActor { .. } => {
                "class = $selection.value.class AND draft.actor.kind = 'service' AND draft.actor.principal = $selection.value.actor AND draft.actor.delegating_principal = $selection.value.delegator AND draft.actor.oauth_client = $selection.value.client"
            }
            Self::DataLabel { .. } => {
                "class = $selection.value.class AND $selection.value.label IN draft.authority.data_labels"
            }
        }
    }
}

pub(crate) struct SmokeAudit {
    store: PlatformStore,
    partitions: Vec<AuditPartition>,
}

impl SmokeAudit {
    pub(crate) async fn connect(
        platform: &PlatformStoreSmoke,
        control_plane: &Path,
    ) -> Result<Self> {
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
                    .query(
                        include_str!("../queries/audit/count.surql")
                            .replace("/* activity */ true", selection.predicate()),
                    )
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

    pub(crate) async fn at_least(
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

    pub(crate) async fn exact(
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
    pub(crate) fn assert_cli(&self, gateway: &Path, platform: &PlatformStoreSmoke) -> Result<()> {
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
