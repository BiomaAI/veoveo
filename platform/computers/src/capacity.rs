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
