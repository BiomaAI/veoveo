//! Named-lane history and transactional execution of fully admitted installations.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::{
    Connection, Surreal,
    types::{RecordId, SurrealValue},
};

use super::{PreparedInstallation, RunnerError};
use crate::{
    LANE_TABLE, LaneRequirement, MIGRATION_TABLE, Migration, MigrationVersion, ModuleName,
    PREPARATION_TABLE, PreparationKey,
};

#[path = "executor/transaction.rs"]
pub(super) mod transaction;
pub use transaction::ExecutionLimits;

const INFRASTRUCTURE: &str = include_str!("../../migrations/0000_lane_bookkeeping.surql");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneStatus {
    pub module: ModuleName,
    pub initialized: bool,
    pub current: Option<MigrationVersion>,
    pub latest: Option<MigrationVersion>,
    pub pending: Vec<MigrationVersion>,
    pub selected: bool,
}
impl LaneStatus {
    pub fn is_current(&self) -> bool {
        self.initialized && self.pending.is_empty() && self.current == self.latest
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationStatus {
    pub lanes: Vec<LaneStatus>,
}
impl InstallationStatus {
    pub fn is_current(&self) -> bool {
        self.lanes
            .iter()
            .filter(|lane| lane.selected)
            .all(LaneStatus::is_current)
    }
    pub fn lane(&self, module: &ModuleName) -> Option<&LaneStatus> {
        self.lanes.iter().find(|lane| &lane.module == module)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, SurrealValue)]
pub(super) struct Header {
    id: RecordId,
    module: String,
    initialized: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, SurrealValue)]
struct Receipt {
    module: String,
    version: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize, SurrealValue)]
pub(super) struct Applied {
    id: RecordId,
    module: String,
    version: i64,
    name: String,
    filename: String,
    checksum: String,
    requires_checksum: String,
    requirements: Vec<Receipt>,
}
fn header_id(module: &ModuleName) -> RecordId {
    RecordId::new(LANE_TABLE, module.as_str())
}
fn migration_id(module: &ModuleName, version: MigrationVersion) -> RecordId {
    RecordId::new(
        MIGRATION_TABLE,
        format!("{}:{}", module.as_str(), version.get()),
    )
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn requires_checksum(migration: &Migration) -> String {
    // Length framing and a presence tag keep None distinct from migration zero.
    let mut bytes = Vec::new();
    for required in migration.requires() {
        let name = required.module().as_str().as_bytes();
        bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
        bytes.extend_from_slice(name);
        match required.minimum() {
            None => bytes.push(0),
            Some(version) => {
                bytes.push(1);
                bytes.extend_from_slice(&version.get().to_be_bytes());
            }
        }
    }
    digest(&bytes)
}
fn failure(operation: &str, module: Option<&ModuleName>) -> RunnerError {
    RunnerError::new(match module {
        Some(module) => format!("{operation} for module {module}"),
        None => operation.to_owned(),
    })
}

impl PreparedInstallation<'_> {
    pub(super) fn validate_history(
        &self,
        headers: &[Header],
        applied: &[Applied],
    ) -> Result<InstallationStatus, RunnerError> {
        let registry = self.selection().registry();
        let mut initialized = BTreeMap::new();
        for header in headers {
            let name = ModuleName::new(header.module.clone())
                .map_err(|_| failure("invalid lane identity", None))?;
            if registry.module(&name).is_none()
                || header.id != header_id(&name)
                || !header.initialized
                || initialized.insert(name, ()).is_some()
            {
                return Err(failure("unknown or invalid persisted lane", None));
            }
        }
        let mut histories: BTreeMap<ModuleName, BTreeMap<MigrationVersion, &Applied>> =
            BTreeMap::new();
        for row in applied {
            let name = ModuleName::new(row.module.clone())
                .map_err(|_| failure("invalid migration lane identity", None))?;
            let module = registry
                .module(&name)
                .ok_or_else(|| failure("unknown persisted migration lane", None))?;
            let version = u32::try_from(row.version)
                .map(MigrationVersion::new)
                .map_err(|_| failure("invalid persisted migration version", Some(&name)))?;
            if !initialized.contains_key(&name)
                || row.id != migration_id(&name, version)
                || histories
                    .entry(name.clone())
                    .or_default()
                    .insert(version, row)
                    .is_some()
            {
                return Err(failure(
                    "invalid migration identity or missing lane header",
                    Some(&name),
                ));
            }
            let expected = module
                .lane()
                .migrations()
                .get(version.get() as usize)
                .ok_or_else(|| {
                    failure("database migration is ahead of compiled lane", Some(&name))
                })?;
            if row.name != expected.name().as_str()
                || row.filename != expected.filename()
                || row.checksum != digest(expected.sql().as_bytes())
                || row.requires_checksum != requires_checksum(expected)
            {
                return Err(failure("migration history drift", Some(&name)));
            }
        }
        let mut lanes = Vec::new();
        for module in registry.ordered() {
            let history = histories.get(module.name());
            let current = history.and_then(|history| history.keys().next_back().copied());
            if let Some(current) = current {
                for version in 0..=current.get() {
                    if !history
                        .expect("current implies history")
                        .contains_key(&MigrationVersion::new(version))
                    {
                        return Err(failure("migration history gap", Some(module.name())));
                    }
                }
            }
            lanes.push(LaneStatus {
                module: module.name().clone(),
                initialized: initialized.contains_key(module.name()),
                current,
                latest: module.lane().latest(),
                pending: module
                    .lane()
                    .migrations()
                    .iter()
                    .map(Migration::version)
                    .filter(|version| history.is_none_or(|history| !history.contains_key(version)))
                    .collect(),
                selected: self.selection().contains(module.name()),
            });
        }
        let status = InstallationStatus { lanes };
        for row in applied {
            let name = ModuleName::new(row.module.clone())
                .map_err(|_| failure("invalid migration lane identity", None))?;
            let module = registry.module(&name).expect("validated module");
            let expected = &module.lane().migrations()[row.version as usize];
            if row.requirements.len() != expected.requires().len() {
                return Err(failure(
                    "migration prerequisite receipt mismatch",
                    Some(&name),
                ));
            }
            let mut seen = BTreeMap::new();
            for (receipt, requirement) in row.requirements.iter().zip(expected.requires()) {
                if receipt.module != requirement.module().as_str()
                    || seen.insert(&receipt.module, ()).is_some()
                {
                    return Err(failure(
                        "migration prerequisite receipt mismatch",
                        Some(&name),
                    ));
                }
                let lane = status
                    .lane(requirement.module())
                    .ok_or_else(|| failure("unknown prerequisite receipt", Some(&name)))?;
                let recorded = receipt
                    .version
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| failure("invalid prerequisite receipt version", Some(&name)))?
                    .map(MigrationVersion::new);
                if !lane.initialized
                    || recorded
                        .is_some_and(|version| lane.current.is_none_or(|current| current < version))
                    || requirement
                        .minimum()
                        .is_some_and(|minimum| recorded.is_none_or(|version| version < minimum))
                {
                    return Err(failure("unapplied historical prerequisite", Some(&name)));
                }
            }
        }
        Ok(status)
    }
    pub async fn status<C: Connection>(
        &self,
        db: &Surreal<C>,
    ) -> Result<InstallationStatus, RunnerError> {
        self.status_with_limits(db, ExecutionLimits::default())
            .await
    }
    pub async fn status_with_limits<C: Connection>(
        &self,
        db: &Surreal<C>,
        limits: ExecutionLimits,
    ) -> Result<InstallationStatus, RunnerError> {
        if !infrastructure_state(db, limits).await? {
            return self.validate_history(&[], &[]);
        }
        match transaction::execute(db, transaction::Operation::History, limits).await? {
            transaction::Output::History(headers, applied) => {
                self.validate_history(&headers, &applied)
            }
            transaction::Output::Written => unreachable!("history operation returns history"),
        }
    }
    /// Initialize only reserved infrastructure after complete selected-SQL admission.
    pub async fn initialize<C: Connection>(&self, db: &Surreal<C>) -> Result<(), RunnerError> {
        let limits = ExecutionLimits::default();
        if infrastructure_state(db, limits).await? {
            return Ok(());
        }
        let result = transaction::execute(db, transaction::Operation::Infrastructure, limits).await;
        if let Err(error) = result
            && !error.may_observe_winner
        {
            return Err(error.error);
        }
        if !infrastructure_state(db, limits).await? {
            return Err(failure(
                "no matching committed bookkeeping infrastructure",
                None,
            ));
        }
        Ok(())
    }

    pub async fn apply<C: Connection>(
        &self,
        db: &Surreal<C>,
    ) -> Result<InstallationStatus, RunnerError> {
        self.apply_with_limits(db, ExecutionLimits::default()).await
    }
    pub async fn apply_with_limits<C: Connection>(
        &self,
        db: &Surreal<C>,
        limits: ExecutionLimits,
    ) -> Result<InstallationStatus, RunnerError> {
        self.status_with_limits(db, limits).await?;
        for module in self.selection().ordered() {
            self.apply_lane_with_limits(db, module.name(), limits)
                .await?;
        }
        self.status_with_limits(db, limits).await
    }
    pub async fn apply_lane<C: Connection>(
        &self,
        db: &Surreal<C>,
        name: &ModuleName,
    ) -> Result<InstallationStatus, RunnerError> {
        self.apply_lane_with_limits(db, name, ExecutionLimits::default())
            .await
    }
    pub async fn apply_lane_with_limits<C: Connection>(
        &self,
        db: &Surreal<C>,
        name: &ModuleName,
        limits: ExecutionLimits,
    ) -> Result<InstallationStatus, RunnerError> {
        self.apply_lane_guarded(db, name, limits, None).await
    }
    /// Installation Jobs fence every native mutation against the prepared generation.
    pub async fn apply_installation_lane<C: Connection>(
        &self,
        db: &Surreal<C>,
        name: &ModuleName,
        preparation: &PreparationKey,
    ) -> Result<InstallationStatus, RunnerError> {
        self.require_preparation(db, preparation).await?;
        self.apply_lane_guarded(db, name, ExecutionLimits::default(), Some(preparation))
            .await
    }
    async fn apply_lane_guarded<C: Connection>(
        &self,
        db: &Surreal<C>,
        name: &ModuleName,
        limits: ExecutionLimits,
        preparation: Option<&PreparationKey>,
    ) -> Result<InstallationStatus, RunnerError> {
        if !self.selection().contains(name) {
            return Err(failure("lane is not enabled", Some(name)));
        }
        let module = self
            .selection()
            .registry()
            .module(name)
            .expect("selected module");
        let status = self.status_with_limits(db, limits).await?;
        let lane = status.lane(name).expect("known lane");
        if lane.is_current() {
            if let Some(key) = preparation {
                self.require_preparation(db, key).await?;
            }
            return Ok(status);
        }
        check_prerequisites(module.requires(), &status, name)?;
        if !infrastructure_state(db, limits).await? {
            let result =
                transaction::execute(db, transaction::Operation::Infrastructure, limits).await;
            // Both success and uncertainty require authoritative committed schema validation.
            if !infrastructure_state(db, limits).await? {
                return Err(failure(
                    "no matching committed bookkeeping infrastructure",
                    None,
                ));
            }
            // A matching committed infrastructure winner settles a concurrent initializer.
            if let Err(error) = result
                && !error.may_observe_winner
            {
                return Err(error.error);
            }
        }
        if !lane.initialized {
            let result = transaction::execute(
                db,
                transaction::Operation::Header {
                    content: Header {
                        id: header_id(name),
                        module: name.as_str().into(),
                        initialized: true,
                    },
                    preparation: preparation.cloned(),
                },
                limits,
            )
            .await;
            if let Err(error) = result {
                if !error.may_observe_winner {
                    return Err(error.error);
                }
                if !self
                    .status_with_limits(db, limits)
                    .await?
                    .lane(name)
                    .expect("known lane")
                    .initialized
                {
                    return Err(failure(
                        &format!(
                            "lane initialization failed without a matching committed winner: {}",
                            error.error
                        ),
                        Some(name),
                    ));
                }
            }
        }
        for migration in module.lane().migrations() {
            let status = self.status_with_limits(db, limits).await?;
            if !status
                .lane(name)
                .expect("known lane")
                .pending
                .contains(&migration.version())
            {
                continue;
            }
            check_prerequisites(module.requires(), &status, name)?;
            check_prerequisites(migration.requires(), &status, name)?;
            let content = Applied {
                id: migration_id(name, migration.version()),
                module: name.as_str().into(),
                version: i64::from(migration.version().get()),
                name: migration.name().as_str().into(),
                filename: migration.filename().into(),
                checksum: digest(migration.sql().as_bytes()),
                requires_checksum: requires_checksum(migration),
                requirements: migration
                    .requires()
                    .iter()
                    .map(|requirement| Receipt {
                        module: requirement.module().as_str().into(),
                        version: status
                            .lane(requirement.module())
                            .expect("checked prerequisite")
                            .current
                            .map(|version| i64::from(version.get())),
                    })
                    .collect(),
            };
            if let Err(error) = transaction::execute(
                db,
                transaction::Operation::Migration {
                    sql: migration.sql(),
                    content,
                    preparation: preparation.cloned(),
                },
                limits,
            )
            .await
            {
                if !error.may_observe_winner {
                    return Err(error.error);
                }
                let status = self.status_with_limits(db, limits).await?;
                if status
                    .lane(name)
                    .expect("known lane")
                    .pending
                    .contains(&migration.version())
                {
                    return Err(failure(
                        &format!(
                            "migration {} failed without a matching committed winner: {}",
                            migration.version().get(),
                            error.error
                        ),
                        Some(name),
                    ));
                }
            }
        }
        if let Some(key) = preparation {
            self.require_preparation(db, key).await?;
        }
        self.status_with_limits(db, limits).await
    }
}

#[derive(Deserialize, SurrealValue)]
struct DatabaseInfo {
    tables: BTreeMap<String, String>,
}
#[derive(Deserialize, SurrealValue)]
struct TableInfo {
    fields: BTreeMap<String, String>,
    indexes: BTreeMap<String, String>,
    events: BTreeMap<String, String>,
}
pub(super) async fn infrastructure_state<C: Connection>(
    db: &Surreal<C>,
    limits: ExecutionLimits,
) -> Result<bool, RunnerError> {
    let limits = limits.check()?;
    let mut response = tokio::time::timeout(
        limits.operation_timeout,
        db.query(include_str!("../../queries/database_info.surql")),
    )
    .await
    .map_err(|_| failure("database readiness inspection timed out", None))?
    .map_err(|_| failure("cannot inspect ready database", None))?;
    let info = response
        .take::<Option<DatabaseInfo>>(0)
        .map_err(|_| failure("cannot inspect ready database", None))?
        .ok_or_else(|| failure("ready database inspection returned no metadata", None))?;
    let count = [LANE_TABLE, MIGRATION_TABLE, PREPARATION_TABLE]
        .iter()
        .filter(|table| info.tables.contains_key(**table))
        .count();
    if count == 0 {
        return Ok(false);
    }
    if count != 3 {
        return Err(failure(
            "incomplete bookkeeping schema; repair explicitly",
            None,
        ));
    }
    let mut actual = vec![
        info.tables[LANE_TABLE].clone(),
        info.tables[MIGRATION_TABLE].clone(),
        info.tables[PREPARATION_TABLE].clone(),
    ];
    for sql in [
        include_str!("../../queries/lane_info.surql"),
        include_str!("../../queries/migration_info.surql"),
        include_str!("../../queries/preparation_info.surql"),
    ] {
        let mut response = tokio::time::timeout(limits.operation_timeout, db.query(sql))
            .await
            .map_err(|_| failure("bookkeeping schema inspection timed out", None))?
            .map_err(|_| failure("cannot inspect bookkeeping schema", None))?;
        let info = response
            .take::<Option<TableInfo>>(0)
            .map_err(|_| failure("invalid bookkeeping schema metadata", None))?
            .ok_or_else(|| failure("bookkeeping inspection returned no metadata", None))?;
        if !info.events.is_empty() {
            return Err(failure(
                "unexpected bookkeeping events; repair explicitly",
                None,
            ));
        }
        actual.extend(info.fields.into_values());
        actual.extend(info.indexes.into_values());
    }
    // This owned resource has exactly one complete definition per line. Pinned AST
    // comparison performs syntax/semantic matching; this does not parse arbitrary SQL.
    let expected: Vec<_> = INFRASTRUCTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if actual.len() != expected.len() {
        return Err(failure(
            &format!(
                "incompatible bookkeeping definition count (actual {}, expected {}); repair explicitly",
                actual.len(),
                expected.len()
            ),
            None,
        ));
    }
    for expected in expected {
        let mut matching = None;
        for (index, actual) in actual.iter().enumerate() {
            if super::policy::same_definition(actual, expected)? {
                matching = Some(index);
                break;
            }
        }
        let Some(index) = matching else {
            return Err(failure(
                "incompatible bookkeeping definition; repair explicitly",
                None,
            ));
        };
        actual.remove(index);
    }
    Ok(true)
}
fn check_prerequisites(
    requirements: &[LaneRequirement],
    status: &InstallationStatus,
    module: &ModuleName,
) -> Result<(), RunnerError> {
    for requirement in requirements {
        let lane = status
            .lane(requirement.module())
            .ok_or_else(|| failure("unknown lane prerequisite", Some(module)))?;
        let satisfied = match requirement.minimum() {
            None => lane.is_current(),
            Some(minimum) => {
                lane.initialized && lane.current.is_some_and(|version| version >= minimum)
            }
        };
        if !satisfied {
            return Err(failure("lane prerequisite has not completed", Some(module)));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
