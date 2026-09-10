use crate::{CapacityPolicy, ComputerError, ComputersStore, Result, identity::digest};
use serde::Deserialize;
use surrealdb::types::{RecordId, SurrealValue};

#[derive(Deserialize, SurrealValue)]
struct StoredCapacity {
    per_owner: u32,
    per_tenant: u32,
    provider: u32,
}

impl ComputersStore {
    /// Current quota hint for the collection. The reservation transaction still
    /// owns admission and arbitrates races against these counts and policy.
    pub async fn capacity_for(
        &self,
        owner: &veoveo_task_runtime::TaskOwner,
    ) -> Result<(CapacityPolicy, bool)> {
        let capacity = self.capacity().await?;
        let tenant = veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())
            .map_err(|_| ComputerError::InvalidInput)?;
        let mut response = self.query(
            "SELECT VALUE retained FROM ONLY $owner; SELECT VALUE retained FROM ONLY $tenant; SELECT VALUE retained FROM ONLY $provider;",
            vec![
                ("owner", RecordId::new("computer_usage", format!("owner:{}", crate::identity::quota_key(owner)?)).into_value()),
                ("tenant", RecordId::new("computer_usage", format!("tenant:{tenant}")).into_value()),
                ("provider", RecordId::new("computer_usage", format!("provider:{}", self.provider_instance_id)).into_value()),
            ],
        ).await?;
        let owner: Option<u64> = response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let tenant: Option<u64> = response.take(1).map_err(|_| ComputerError::Unavailable)?;
        let provider: Option<u64> = response.take(2).map_err(|_| ComputerError::Unavailable)?;
        let available = owner.unwrap_or(0) < u64::from(capacity.per_owner)
            && tenant.unwrap_or(0) < u64::from(capacity.per_tenant)
            && provider.unwrap_or(0) < u64::from(capacity.provider);
        Ok((capacity, available))
    }

    pub(crate) fn capacity_record(&self) -> RecordId {
        RecordId::new(
            "computer_capacity",
            surrealdb::types::Uuid::from(self.provider_instance_id),
        )
    }

    /// Installation administration only. Compare-and-set prevents a stale installer
    /// from restoring an older limit. There is no public Computer request for this.
    pub async fn install_capacity(
        &self,
        expected: Option<CapacityPolicy>,
        next: CapacityPolicy,
    ) -> Result<()> {
        self.query(
            include_str!("../queries/capacity.surql"),
            vec![
                ("capacity", self.capacity_record().into_value()),
                (
                    "expected",
                    expected.map(|p| digest(&p)).transpose()?.into_value(),
                ),
                ("fingerprint", digest(&next)?.into_value()),
                ("owner_limit", i64::from(next.per_owner).into_value()),
                ("tenant_limit", i64::from(next.per_tenant).into_value()),
                ("provider_limit", i64::from(next.provider).into_value()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn capacity(&self) -> Result<CapacityPolicy> {
        let mut response = self
            .query(
                "SELECT * FROM ONLY $capacity;",
                vec![("capacity", self.capacity_record().into_value())],
            )
            .await?;
        let capacity: Option<StoredCapacity> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let capacity = capacity.ok_or(ComputerError::Unavailable)?;
        Ok(CapacityPolicy {
            per_owner: capacity.per_owner,
            per_tenant: capacity.per_tenant,
            provider: capacity.provider,
        })
    }
}
