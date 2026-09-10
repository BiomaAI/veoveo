use crate::{ComputerError, ComputersStore, Result, identity::digest};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

/// Installation-owned limits. Zero max_grants closes new admission and renewal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionGrantPolicy {
    pub max_grants: u32,
    pub absolute_seconds: u32,
    pub idle_seconds: u32,
}
#[derive(Deserialize, SurrealValue)]
pub(super) struct StoredPolicy {
    pub fingerprint: String,
    pub max_grants: u32,
    pub absolute_seconds: u32,
    pub idle_seconds: u32,
}
impl StoredPolicy {
    pub fn checked(&self) -> Result<SessionGrantPolicy> {
        let policy = SessionGrantPolicy {
            max_grants: self.max_grants,
            absolute_seconds: self.absolute_seconds,
            idle_seconds: self.idle_seconds,
        };
        policy.validate()?;
        if digest(&policy)? != self.fingerprint {
            return Err(ComputerError::Unavailable);
        }
        Ok(policy)
    }
}
impl SessionGrantPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.max_grants > 128
            || !(1..=86400).contains(&self.absolute_seconds)
            || self.idle_seconds == 0
            || self.idle_seconds > self.absolute_seconds
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}
impl ComputersStore {
    pub async fn session_grant_policy(&self) -> Result<SessionGrantPolicy> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut response = self
                .query(
                    "SELECT * FROM ONLY $policy;",
                    vec![("policy", self.session_policy_record().into_value())],
                )
                .await?;
            let policy: Option<StoredPolicy> =
                response.take(0).map_err(|_| ComputerError::Unavailable)?;
            policy.ok_or(ComputerError::Unavailable)?.checked()
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    pub(super) fn session_policy_record(&self) -> RecordId {
        RecordId::new(
            "computer_session_grant_policy",
            surrealdb::types::Uuid::from(self.provider_instance_id),
        )
    }
    pub async fn install_session_grant_policy(
        &self,
        expected: Option<SessionGrantPolicy>,
        next: SessionGrantPolicy,
    ) -> Result<()> {
        next.validate()?;
        self.query(
            include_str!("../../queries/session_grant_policy.surql"),
            vec![
                ("policy", self.session_policy_record().into_value()),
                (
                    "expected",
                    expected.map(|p| digest(&p)).transpose()?.into_value(),
                ),
                ("fingerprint", digest(&next)?.into_value()),
                ("max_grants", next.max_grants.into_value()),
                ("absolute_seconds", next.absolute_seconds.into_value()),
                ("idle_seconds", next.idle_seconds.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
