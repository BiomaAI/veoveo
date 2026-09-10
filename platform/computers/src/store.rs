use crate::{Computer, ComputerError, ComputerPage, Result, identity::*, model::*};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::PlatformStore;
use veoveo_task_runtime::TaskOwner;

#[derive(Clone)]
pub struct ComputersStore {
    pub(crate) platform: PlatformStore,
    pub(crate) provider_instance_id: Uuid,
}

impl ComputersStore {
    pub fn new(platform: PlatformStore, provider_instance_id: Uuid) -> Result<Self> {
        if provider_instance_id.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Self {
            platform,
            provider_instance_id,
        })
    }

    pub async fn get(&self, caller: &TaskOwner, id: Uuid) -> Result<Computer> {
        owner_key(caller)?;
        let mut response = self
            .query(
                "SELECT * FROM ONLY $computer;",
                vec![("computer", computer_record(id).into_value())],
            )
            .await?;
        let row: Option<ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let computer = Computer::try_from(row.ok_or(ComputerError::NotFound)?)?;
        permits(&computer.owner, caller)?;
        Ok(computer)
    }

    pub async fn list(
        &self,
        caller: &TaskOwner,
        after: Option<Uuid>,
        limit: u32,
    ) -> Result<ComputerPage> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                include_str!("../queries/list.surql"),
                vec![
                    ("owner", owner_key(caller)?.into_value()),
                    ("after", after.into_value()),
                    ("limit", i64::from(limit + 1).into_value()),
                ],
            )
            .await?;
        let records: Vec<ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut computers = records
            .into_iter()
            .map(Computer::try_from)
            .collect::<Result<Vec<_>>>()?;
        // Never silently drop rows when authority is insufficient; that could produce an
        // apparently complete collection that hides retained, quota-consuming work.
        for computer in &computers {
            permits(&computer.owner, caller)?;
        }
        let more = computers.len() > limit as usize;
        computers.truncate(limit as usize);
        let next_cursor = more.then(|| computers.last().expect("nonzero limit").computer_id);
        Ok(ComputerPage {
            computers,
            next_cursor,
        })
    }

    pub(crate) async fn query(
        &self,
        sql: &'static str,
        bindings: Vec<(&'static str, Value)>,
    ) -> Result<surrealdb::IndexedResults> {
        for attempt in 0..8_u32 {
            let mut query = self.platform.client().query(sql);
            for (key, value) in &bindings {
                query = query.bind((key.to_string(), value.clone()));
            }
            let mut response = query.await.map_err(|_| ComputerError::Unavailable)?;
            let errors = response.take_errors();
            if errors.is_empty() {
                return Ok(response);
            }
            // An aborted transaction produces secondary "not executed" errors. Only an
            // actual conflict authorizes retry; never retry an unknown submission outcome.
            if errors
                .values()
                .any(|e| e.is_thrown() && e.message().contains("computer_capacity"))
            {
                return Err(ComputerError::CapacityFull);
            }
            if errors
                .values()
                .any(|e| e.is_thrown() && e.message().contains("computer_request_conflict"))
            {
                return Err(ComputerError::RequestConflict);
            }
            let conflict = errors.values().any(|e| {
                matches!(
                    e.query_details(),
                    Some(surrealdb::types::QueryError::TransactionConflict)
                ) || e.message().starts_with("Transaction conflict:")
            });
            if errors
                .values()
                .any(|e| e.is_thrown() && e.message().contains("computer_policy_conflict"))
            {
                return Err(ComputerError::PolicyConflict);
            }
            if !conflict || attempt == 7 {
                return Err(ComputerError::Unavailable);
            }
            tokio::time::sleep(std::time::Duration::from_millis(1 << attempt)).await;
        }
        Err(ComputerError::Unavailable)
    }
}
