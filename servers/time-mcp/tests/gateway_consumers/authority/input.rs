use super::*;
use veoveo_types::{PrincipalId, TenantId, WorkContextId};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Isolation {
    pub namespace: String,
    pub tenant_id: TenantId,
    pub work_context: WorkContextId,
    pub reader_principal_id: PrincipalId,
    pub administrator_principal_id: PrincipalId,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceFixture {
    pub create: CreateSourceRequest,
    pub digest: AuthoritySourceDigest,
    pub expected_version_label: String,
    pub acquisition_keys: [String; 3],
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    pub installation: installed::InstalledSource,
    pub administrator: super::super::Administrator,
    /// Ops attests a dedicated installation and these verified OAuth identities.
    pub isolation: Isolation,
    pub initial_authority: EffectiveTimeAuthority,
    pub tzdb: SourceFixture,
    pub leaps: SourceFixture,
    pub epoch: MissionEpoch,
    pub epoch_idempotency_keys: [String; 2],
    pub relative_offset_nanoseconds: i64,
    pub expected_relative_tai_nanoseconds: i128,
    pub expected_utc: String,
}
impl Input {
    pub fn admit(&self, target: &veoveo_deploy_contract::InstallationTarget) -> Result<()> {
        ensure!(
            self.isolation.namespace == target.kubernetes.namespace
                && self.isolation.work_context.as_str() == target.operator.work_context,
            "authority fixture must select the attested isolated installation/context"
        );
        ensure!(
            self.tzdb.create.source.dataset_kind == AuthorityDatasetKind::Tzdb
                && self.leaps.create.source.dataset_kind == AuthorityDatasetKind::LeapSeconds
                && self.tzdb.create.source.source_id != self.leaps.create.source.source_id
                && self.tzdb.create.source.enabled
                && self.leaps.create.source.enabled,
            "authority fixture requires distinct enabled TZDB/leap sources"
        );
        ensure!(
            self.epoch.version.get() == 1
                && self.epoch.instant.authority == self.initial_authority.binding(),
            "epoch v1 must bind the independently selected initial authority"
        );
        ensure!(
            self.expected_relative_tai_nanoseconds
                == self
                    .epoch
                    .instant
                    .total_nanoseconds()
                    .checked_add(i128::from(self.relative_offset_nanoseconds))
                    .context("epoch arithmetic overflow")?,
            "independent relative expectation disagrees with epoch arithmetic"
        );
        ensure!(
            !self.expected_utc.is_empty() && self.expected_utc.len() <= 128,
            "an independent UTC projection is required"
        );
        TimeExpressionValue::Rfc3339 {
            value: self.expected_utc.clone(),
        }
        .build()?;
        for source in [&self.tzdb, &self.leaps] {
            ensure!(
                !source.expected_version_label.trim().is_empty()
                    && source.expected_version_label.len() <= 128,
                "independent authority version labels must be bounded and nonempty"
            );
        }
        let mut keys = std::collections::BTreeSet::new();
        for key in self
            .tzdb
            .acquisition_keys
            .iter()
            .chain(&self.leaps.acquisition_keys)
            .chain(&self.epoch_idempotency_keys)
            .chain([
                &self.tzdb.create.idempotency_key,
                &self.leaps.create.idempotency_key,
            ])
        {
            ensure!(
                !key.trim().is_empty() && key.len() <= 128 && keys.insert(key),
                "authority fixture mutation keys must be distinct and bounded"
            );
        }
        Ok(())
    }
}
