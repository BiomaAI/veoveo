use crate::*;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::PrincipalId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditOrder {
    #[default]
    OldestFirst,
    NewestFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditCursor {
    pub order: AuditOrder,
    pub partition: AuditPartition,
    pub last_id: AuditRecordId,
}
/// The policy owner constructs the scope from authenticated current authority.
#[derive(Debug, Clone)]
pub struct AuditReadScope {
    tenant: Option<veoveo_types::TenantId>,
    installation: bool,
}
impl AuditReadScope {
    pub fn new(tenant: Option<veoveo_types::TenantId>, installation: bool) -> Self {
        Self {
            tenant,
            installation,
        }
    }
    pub fn permits(&self, partition: &AuditPartition) -> bool {
        match partition {
            AuditPartition::Installation => self.installation,
            AuditPartition::Tenant(tenant) => self.tenant.as_ref() == Some(tenant),
        }
    }
    pub fn partitions(&self) -> Vec<AuditPartition> {
        self.tenant
            .clone()
            .map(AuditPartition::Tenant)
            .into_iter()
            .chain(self.installation.then_some(AuditPartition::Installation))
            .collect()
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    pub order: AuditOrder,
    pub partition: AuditPartition,
    pub cursor: Option<AuditCursor>,
    pub class: Option<AuditClass>,
    pub actor: Option<PrincipalId>,
    pub target: Option<AuditTarget>,
    pub outcome: Option<AuditOutcome>,
    pub trace: Option<AuditTraceId>,
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: u16,
}
impl AuditQuery {
    pub fn new(partition: AuditPartition) -> Self {
        Self {
            order: AuditOrder::OldestFirst,
            partition,
            cursor: None,
            class: None,
            actor: None,
            target: None,
            outcome: None,
            trace: None,
            from: None,
            until: None,
            limit: 100,
        }
    }
    pub fn validate(&self) -> Result<(), AuditValidationError> {
        if self.limit == 0 || self.limit > 1000 {
            return Err(AuditValidationError::PageSize);
        }
        if self
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.partition != self.partition || cursor.order != self.order)
        {
            return Err(AuditValidationError::Cursor);
        }
        if self
            .from
            .zip(self.until)
            .is_some_and(|(from, until)| from >= until)
        {
            return Err(AuditValidationError::TimeRange);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AuditPage {
    pub records: Vec<AuditRecord>,
    pub next: Option<AuditCursor>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditDailyCount {
    pub partition: AuditPartition,
    pub day: DateTime<Utc>,
    pub class: AuditClass,
    pub outcome: AuditOutcome,
    pub count: u64,
}

/// Stable keyset over one partition's grouped counts. Date bounds are inclusive/exclusive.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditDailyCursor {
    pub partition: AuditPartition,
    pub day: DateTime<Utc>,
    pub class: AuditClass,
    pub outcome: AuditOutcome,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditDailyQuery {
    pub partition: AuditPartition,
    pub from: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub cursor: Option<AuditDailyCursor>,
    pub limit: u16,
}
impl AuditDailyQuery {
    pub fn validate(&self) -> Result<(), AuditValidationError> {
        if self.limit == 0 || self.limit > 1000 {
            return Err(AuditValidationError::PageSize);
        }
        if self.from >= self.until {
            return Err(AuditValidationError::TimeRange);
        }
        if self.from.time() != chrono::NaiveTime::MIN
            || self.until.time() != chrono::NaiveTime::MIN
            || self
                .cursor
                .as_ref()
                .is_some_and(|c| c.day.time() != chrono::NaiveTime::MIN)
        {
            return Err(AuditValidationError::DayBoundary);
        }
        if self.cursor.as_ref().is_some_and(|cursor| {
            cursor.partition != self.partition || cursor.day < self.from || cursor.day >= self.until
        }) {
            return Err(AuditValidationError::Cursor);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditDailyPage {
    pub counts: Vec<AuditDailyCount>,
    pub next: Option<AuditDailyCursor>,
}
