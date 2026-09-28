//! Vehicle exclusion and plan state change commit in one database transaction.
use super::*;
use crate::contract::MissionId;
use veoveo_types::{PrincipalId, TenantId, WorkContextId};

const COMMAND_LEASE_TTL: Duration = Duration::hours(1);
const ADMIT: &str = include_str!("execution/admit.surql");
const FINISH: &str = include_str!("execution/finish.surql");
pub(super) const EXECUTING_PLANS: &str = include_str!("execution/busy.surql");

#[derive(Clone, Debug)]
struct ExecutionScope {
    tenant: TenantId,
    context: WorkContextId,
    principal: PrincipalId,
}

impl ExecutionScope {
    fn records(&self) -> Result<(RecordId, RecordId)> {
        Ok((
            deterministic_tenant_id(self.tenant.as_str())?.record_id(),
            deterministic_work_context_id(self.tenant.as_str(), self.context.as_str())?.record_id(),
        ))
    }
}

#[derive(Clone, Debug)]
struct CommandLeaseToken(Uuid);

#[derive(Debug)]
pub(in crate::server) struct MissionExecutionGuard {
    plan_record_id: RecordId,
    plan_id: MissionPlanId,
    lease_record_id: RecordId,
    token: CommandLeaseToken,
    scope: ExecutionScope,
    session: SessionId,
    vehicle: VehicleId,
    mission: MissionId,
}

impl MissionExecutionGuard {
    pub(in crate::server) fn dispatch(self) -> DispatchedMission {
        DispatchedMission(self)
    }
}

/// Dispatch consumes the right to release authority on a local setup error.
#[derive(Debug)]
pub(in crate::server) struct DispatchedMission(MissionExecutionGuard);

impl DispatchedMission {
    pub(in crate::server) fn plan_id(&self) -> &MissionPlanId {
        &self.0.plan_id
    }
}

#[derive(Clone, Copy)]
pub(super) enum Settlement {
    Completed,
    NotDispatched,
}

/// Checked caller and retained plan, before the transaction rechecks current authority.
pub(super) struct ExecutionDraft {
    record: PlanRecord,
    plan: VehicleMissionPlan,
    lease_record_id: RecordId,
    scope: ExecutionScope,
}

impl VehicleControlAuthority {
    pub(in crate::server) async fn begin_execution(
        &self,
        identity: &GatewayInternalIdentity,
        plan_id: &MissionPlanId,
        expected_revision: u64,
    ) -> Result<(VehicleMissionPlan, MissionExecutionGuard)> {
        let draft = self
            .prepare_execution(identity, plan_id, expected_revision)
            .await?;
        self.admit_execution(draft).await
    }

    pub(super) async fn prepare_execution(
        &self,
        identity: &GatewayInternalIdentity,
        plan_id: &MissionPlanId,
        expected_revision: u64,
    ) -> Result<ExecutionDraft> {
        let record_id = scoped_record_id("uav_vehicle_mission_plan", identity, plan_id.as_str());
        let record = self.plan_record(&record_id).await?;
        let (tenant, context) = context_records(identity)?;
        if record.tenant != tenant
            || record.work_context != context
            || record.principal_key != identity.actor.id.as_str()
            || record.state != "prepared"
            || record.revision != checked_i64(expected_revision)?
            || record.expires_at <= Utc::now()
        {
            return Err(ControlAuthorityError::Conflict);
        }
        let plan = visible_plan_view(&record, identity)?;
        if &plan.plan_id != plan_id {
            return Err(ControlAuthorityError::Conflict);
        }
        self.require_route_permission(
            identity,
            &plan.session_id,
            &plan.vehicle_id,
            VehicleControlPermission::Execute,
            &plan.map_route,
        )
        .await?;
        Ok(ExecutionDraft {
            lease_record_id: vehicle_lease_record_id(identity, &plan.session_id, &plan.vehicle_id),
            record,
            plan,
            scope: ExecutionScope {
                tenant: identity.authority.tenant.clone(),
                context: identity.authority.work_context.clone(),
                principal: identity.actor.id.clone(),
            },
        })
    }

    pub(super) async fn admit_execution(
        &self,
        draft: ExecutionDraft,
    ) -> Result<(VehicleMissionPlan, MissionExecutionGuard)> {
        let ExecutionDraft {
            record,
            mut plan,
            lease_record_id,
            scope,
        } = draft;
        let (tenant, context) = scope.records()?;
        let now = Utc::now();
        let requirement = RouteRequirement::new(&plan.map_route)?;
        let profile = requirement.profile.as_str().to_owned();
        let advisory = requirement.advisory;
        plan.state = MissionPlanLifecycle::Executing;
        plan.revision = plan
            .revision
            .checked_add(1)
            .ok_or(ControlAuthorityError::Conflict)?;
        checked_i64(plan.revision)?;
        plan.updated_at = now;
        let token = CommandLeaseToken(Uuid::now_v7());
        // Only a repository-owned predicate is inserted into this complete statement.
        let query = ADMIT
            .replace("__PERMITTED_GRANT__", reads::PERMITTED)
            .replace("__EXECUTING_PLANS__", EXECUTING_PLANS);
        let response = self
            .store
            .client()
            .query(query)
            .bind(("record", record.id.clone()))
            .bind(("expected_plan", record.clone()))
            .bind(("canonical", serde_json::to_string(&plan)?))
            .bind(("lease", lease_record_id.clone()))
            .bind(("lease_token", token.0.to_string()))
            .bind(("lease_expires", now + COMMAND_LEASE_TTL))
            .bind(("now", now))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", scope.principal.to_string()))
            .bind(("simulation_session", plan.session_id.to_string()))
            .bind(("vehicle", plan.vehicle_id.to_string()))
            .bind(("mission", plan.mission_id.to_string()))
            .bind((
                "permissions",
                permission_strings(&BTreeSet::from([VehicleControlPermission::Execute])),
            ))
            .bind(("profile", profile))
            .bind(("advisory", advisory))
            .bind(("max_revision", i64::MAX))
            .await?;
        check_transaction(response, &plan.vehicle_id)?;
        let guard = MissionExecutionGuard {
            plan_record_id: record.id,
            plan_id: plan.plan_id.clone(),
            lease_record_id,
            token,
            scope,
            session: plan.session_id.clone(),
            vehicle: plan.vehicle_id.clone(),
            mission: plan.mission_id.clone(),
        };
        Ok((plan, guard))
    }

    pub(in crate::server) async fn abort_execution(
        &self,
        guard: &MissionExecutionGuard,
    ) -> Result<()> {
        self.finish_execution(guard, Settlement::NotDispatched)
            .await
    }

    pub(in crate::server) async fn complete_execution(
        &self,
        dispatched: &DispatchedMission,
        receipt: &crate::adapter::CompletedOperation,
    ) -> Result<()> {
        let guard = &dispatched.0;
        if !receipt.confirms_mission(&guard.session, &guard.vehicle, &guard.mission) {
            return Err(ControlAuthorityError::Conflict);
        }
        self.finish_execution(guard, Settlement::Completed).await
    }

    pub(super) async fn finish_execution(
        &self,
        guard: &MissionExecutionGuard,
        settlement: Settlement,
    ) -> Result<()> {
        let record = self.plan_record(&guard.plan_record_id).await?;
        let mut plan = plan_view(&record)?;
        let (tenant, context) = guard.scope.records()?;
        if plan.plan_id != guard.plan_id
            || record.tenant != tenant
            || record.work_context != context
            || plan.principal_key != guard.scope.principal.as_str()
            || plan.session_id != guard.session
            || plan.vehicle_id != guard.vehicle
            || plan.mission_id != guard.mission
        {
            return Err(ControlAuthorityError::Conflict);
        }
        let terminal = match settlement {
            Settlement::Completed => MissionPlanLifecycle::Completed,
            Settlement::NotDispatched => MissionPlanLifecycle::Failed,
        };
        if plan.state != MissionPlanLifecycle::Executing && plan.state != terminal {
            return Err(ControlAuthorityError::Conflict);
        }
        let now = Utc::now();
        if plan.state == MissionPlanLifecycle::Executing {
            plan.state = terminal;
            plan.revision = plan
                .revision
                .checked_add(1)
                .ok_or(ControlAuthorityError::Conflict)?;
            checked_i64(plan.revision)?;
            plan.updated_at = now;
        }
        let response = self
            .store
            .client()
            .query(FINISH)
            .bind(("record", guard.plan_record_id.clone()))
            .bind(("expected_plan", record))
            .bind(("canonical", serde_json::to_string(&plan)?))
            .bind((
                "state",
                match settlement {
                    Settlement::Completed => "completed",
                    Settlement::NotDispatched => "failed",
                },
            ))
            .bind(("lease", guard.lease_record_id.clone()))
            .bind(("lease_token", guard.token.0.to_string()))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", guard.scope.principal.to_string()))
            .bind(("simulation_session", guard.session.to_string()))
            .bind(("vehicle", guard.vehicle.to_string()))
            .bind(("mission", guard.mission.to_string()))
            .bind(("now", now))
            .bind(("max_revision", i64::MAX))
            .await?;
        check_transaction(response, &guard.vehicle)
    }
}

fn check_transaction(mut response: surrealdb::IndexedResults, vehicle: &VehicleId) -> Result<()> {
    if let Some(error) = veoveo_platform_store::primary_transaction_error(response.take_errors()) {
        let message = error.to_string();
        if message.contains("uav_vehicle_busy") {
            return Err(ControlAuthorityError::VehicleBusy(vehicle.to_string()));
        }
        if message.contains("uav_execution_forbidden") {
            return Err(ControlAuthorityError::Forbidden);
        }
        if message.contains("uav_execution_conflict")
            || matches!(
                error.query_details(),
                Some(surrealdb::types::QueryError::TransactionConflict)
            )
        {
            return Err(ControlAuthorityError::Conflict);
        }
        return Err(error.into());
    }
    Ok(())
}
