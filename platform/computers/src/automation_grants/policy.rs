use crate::{ComputerError, ComputersStore, Result, identity::digest};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

/// Installation-owned authority limits; zero max_grants denies new use and issuance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantPolicy {
    pub max_grants: u32,
    pub maximum_lifetime_seconds: u32,
    pub maximum_execution_seconds: u32,
    pub maximum_output_bytes: u32,
}
#[derive(Deserialize, SurrealValue)]
pub(super) struct StoredPolicy {
    pub fingerprint: String,
    max_grants: u32,
    maximum_lifetime_seconds: u32,
    maximum_execution_seconds: u32,
    maximum_output_bytes: u32,
}
impl StoredPolicy {
    pub fn checked(&self) -> Result<AutomationGrantPolicy> {
        let policy = AutomationGrantPolicy {
            max_grants: self.max_grants,
            maximum_lifetime_seconds: self.maximum_lifetime_seconds,
            maximum_execution_seconds: self.maximum_execution_seconds,
            maximum_output_bytes: self.maximum_output_bytes,
        };
        policy.validate()?;
        if digest(&policy)? != self.fingerprint {
            return Err(ComputerError::Unavailable);
        }
        Ok(policy)
    }
}
impl AutomationGrantPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.max_grants > 64
            || !(1..=86400).contains(&self.maximum_lifetime_seconds)
            || !(1..=7200).contains(&self.maximum_execution_seconds)
            || !(1..=67108864).contains(&self.maximum_output_bytes)
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}
impl ComputersStore {
    pub(super) fn automation_policy_record(&self) -> RecordId {
        RecordId::new(
            "computer_automation_policy",
            surrealdb::types::Uuid::from(self.provider_instance_id),
        )
    }
    pub(super) async fn stored_automation_policy(&self) -> Result<StoredPolicy> {
        let mut read = self
            .query(
                "SELECT * FROM ONLY $policy;",
                vec![("policy", self.automation_policy_record().into_value())],
            )
            .await?;
        let policy: Option<StoredPolicy> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let policy = policy.ok_or(ComputerError::Unavailable)?;
        policy.checked()?;
        Ok(policy)
    }
    pub async fn automation_grant_policy(&self) -> Result<AutomationGrantPolicy> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            self.stored_automation_policy().await?.checked()
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    pub async fn install_automation_grant_policy(
        &self,
        expected: Option<AutomationGrantPolicy>,
        next: AutomationGrantPolicy,
    ) -> Result<()> {
        next.validate()?;
        self.query(
            include_str!("../../queries/automation_grant_policy.surql"),
            vec![
                ("policy", self.automation_policy_record().into_value()),
                (
                    "expected",
                    expected.map(|p| digest(&p)).transpose()?.into_value(),
                ),
                ("fingerprint", digest(&next)?.into_value()),
                ("max_grants", next.max_grants.into_value()),
                (
                    "maximum_lifetime_seconds",
                    next.maximum_lifetime_seconds.into_value(),
                ),
                (
                    "maximum_execution_seconds",
                    next.maximum_execution_seconds.into_value(),
                ),
                (
                    "maximum_output_bytes",
                    next.maximum_output_bytes.into_value(),
                ),
            ],
        )
        .await?;
        Ok(())
    }
}
