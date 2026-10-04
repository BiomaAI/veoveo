use crate::{Computer, ComputerError, ComputerPage, Result, identity::*, model::*};
use surrealdb::types::{SurrealValue, Value};
use veoveo_platform_store::PlatformStore;
use veoveo_task_runtime::TaskOwner;

#[derive(Clone)]
pub struct ComputersStore {
    pub(crate) platform: PlatformStore,
    pub(crate) catalog_registry: veoveo_gateway_contract::CatalogRegistry,
    pub(crate) authority_events:
        std::sync::Arc<std::sync::OnceLock<crate::authority_changes::AccessEvents>>,
    pub(crate) provider_instance_id: crate::api::ProviderInstanceId,
}

pub(crate) fn owner_query_bindings(caller: &TaskOwner) -> Result<Vec<(&'static str, Value)>> {
    owner_key(caller)?;
    Ok(vec![
        ("owner_tenant", caller.tenant_key().to_owned().into_value()),
        ("owner_principal", caller.principal_key.clone().into_value()),
        ("owner_kind", caller.principal_kind.into_value()),
        ("owner_issuer", caller.issuer.clone().into_value()),
        ("owner_subject", caller.subject.clone().into_value()),
        (
            "owner_labels",
            caller
                .data_labels
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into_value(),
        ),
        (
            "owner_context",
            caller.authority.work_context.to_string().into_value(),
        ),
    ])
}

impl ComputersStore {
    /// Bounded completion over the same private owner and retained-label boundary.
    pub async fn complete_ids(
        &self,
        caller: &TaskOwner,
        prefix: &str,
    ) -> Result<(Vec<String>, bool)> {
        if prefix.len() > 36
            || !prefix
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-')
        {
            return Err(ComputerError::InvalidInput);
        }
        let mut params = owner_query_bindings(caller)?;
        params.push(("prefix", prefix.to_string().into_value()));
        let mut response = self
            .query(include_str!("../queries/complete.surql"), params)
            .await?;
        let records: Vec<ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut values = Vec::new();
        for record in records {
            let computer = Computer::try_from(record)?;
            permits(&computer.owner, caller)?;
            values.push(computer.computer_id.to_string());
        }
        let more = values.len() > 100;
        values.truncate(100);
        Ok((values, more))
    }
    pub fn provider_instance_id(&self) -> veoveo_computers_contract::ProviderInstanceId {
        self.provider_instance_id
    }
    pub fn new(
        platform: PlatformStore,
        provider_instance_id: crate::api::ProviderInstanceId,
        catalog_registry: veoveo_gateway_contract::CatalogRegistry,
    ) -> Result<Self> {
        Ok(Self {
            platform,
            catalog_registry,
            authority_events: Default::default(),
            provider_instance_id,
        })
    }

    pub async fn get(&self, caller: &TaskOwner, id: crate::api::ComputerId) -> Result<Computer> {
        let mut params = owner_query_bindings(caller)?;
        params.push(("computer", computer_record(id).into_value()));
        let mut response = self
            .query(include_str!("../queries/get.surql"), params)
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
        after: Option<crate::api::ComputerId>,
        limit: u32,
    ) -> Result<ComputerPage> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut params = owner_query_bindings(caller)?;
        params.extend([
            (
                "after",
                after.map(crate::api::ComputerId::as_uuid).into_value(),
            ),
            ("limit", i64::from(limit + 1).into_value()),
        ]);
        let mut response = self
            .query(include_str!("../queries/list.surql"), params)
            .await?;
        let records: Vec<ComputerRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut computers = records
            .into_iter()
            .map(Computer::try_from)
            .collect::<Result<Vec<_>>>()?;
        // SQL admits visibility before LIMIT. These checks detect inconsistent stored
        // identity; they never filter or repair an already selected page.
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
            for (message, error) in [
                ("computer_invalid_input", ComputerError::InvalidInput),
                ("computer_access_limit", ComputerError::AccessLimit),
                ("computer_authority_expired", ComputerError::Forbidden),
                ("computer_not_found", ComputerError::NotFound),
                ("computer_operation_busy", ComputerError::OperationBusy),
                ("computer_state_conflict", ComputerError::StateConflict),
                ("computer_invalid_state", ComputerError::InvalidState),
            ] {
                if errors
                    .values()
                    .any(|e| e.is_thrown() && e.message().contains(message))
                {
                    return Err(error);
                }
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
