//! The public page is selected in SQL after current policy resolves private permits.
use super::{ComputerReadAccess, scope};
use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, ControlAuthority, Result,
    api::ComputerAccessMode,
};
use std::collections::BTreeMap;
use surrealdb::types::SurrealValue;

impl ComputersStore {
    pub(super) async fn select_admitted_computers(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        admitted: Vec<ComputerReadAccess>,
        limit: u32,
    ) -> Result<Vec<ComputerReadAccess>> {
        control.require_actor(actor)?;
        let mut owned = Vec::new();
        let mut grants = Vec::new();
        let mut access = BTreeMap::new();
        for candidate in admitted {
            candidate.check()?;
            let id = candidate.computer.computer_id;
            match candidate.mode {
                ComputerAccessMode::Owner => owned.push(id.into_uuid()),
                ComputerAccessMode::Granted => grants.extend(candidate.read_permits.clone()),
            }
            if access.insert(id, candidate).is_some() {
                return Err(ComputerError::Unavailable);
            }
        }
        let mut params = control.read_bindings()?;
        params.extend(scope(actor.accepted())?);
        params.extend(crate::store::owner_query_bindings(actor.owner())?);
        params.extend([
            ("owned", owned.into_value()),
            ("grants", grants.into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("policy", self.automation_policy_record().into_value()),
            ("limit", limit.into_value()),
        ]);
        let mut read = self
            .query(
                include_str!("../../queries/admitted_computers.surql"),
                params,
            )
            .await?;
        let index = read
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let rows: Vec<crate::model::ComputerRecord> =
            read.take(index).map_err(|_| ComputerError::Unavailable)?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let computer = Computer::try_from(row)?;
            let mut admitted = access
                .remove(&computer.computer_id)
                .ok_or(ComputerError::Unavailable)?;
            // SQL supplies fresh state; a query cannot switch an admission's owner.
            if admitted.computer.owner != computer.owner {
                return Err(ComputerError::StateConflict);
            }
            admitted.computer = computer;
            admitted.check()?;
            result.push(admitted);
        }
        control.require_actor(actor)?;
        Ok(result)
    }
}
