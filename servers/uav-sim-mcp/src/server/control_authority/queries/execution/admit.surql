LET $lookup = (SELECT * FROM ONLY $catalog WHERE task = $task
    AND task_type = $expected_uav_type AND created_at = $expected_uav_created
    AND identity = $expected_uav_identity AND settlement = NONE);
IF type::is_object($lookup) = false { THROW 'uav_execution_conflict'; };

LET $previous_lease = (
  SELECT *
  FROM ONLY $lease
);
IF $previous_lease != NONE
AND (
  $previous_lease.tenant != $tenant
  OR $previous_lease.work_context != $work_context
  OR $previous_lease.session_id != $simulation_session
  OR $previous_lease.vehicle_id != $vehicle
  OR $previous_lease.revision < 0
  OR $previous_lease.revision >= $max_revision
) {
  THROW 'uav_execution_conflict';
};

-- Every contender writes the same vehicle record. On 3.3.0 this supplies the
-- shared write conflict that excludes decisions from concurrent snapshots.
IF $previous_lease = NONE {
  CREATE ONLY $lease CONTENT {
    tenant: $tenant,
    work_context: $work_context,
    session_id: $simulation_session,
    vehicle_id: $vehicle,
    principal_key: $principal,
    mission_id: $mission,
    lease_token: $lease_token,
    expires_at: $lease_expires,
    released_at: NONE,
    revision: 0,
    created_at: $now,
    updated_at: $now,
  } RETURN NONE;
} ELSE {
  UPDATE ONLY $lease SET
    principal_key = $principal,
    mission_id = $mission,
    lease_token = $lease_token,
    expires_at = $lease_expires,
    released_at = NONE,
    revision += 1,
    updated_at = $now
  RETURN NONE;
};

-- Lease expiry does not prove that an admitted simulator operation has stopped.
IF array::len(
  (
    SELECT VALUE id FROM uav_vehicle_mission_plan
WHERE tenant = $tenant AND work_context = $work_context
    AND session_id = $simulation_session AND vehicle_id = $vehicle AND state = 'executing'
LIMIT 1

  )
) != 0 {
  THROW 'uav_vehicle_busy';
};

IF array::len(
  (
    SELECT VALUE id
    FROM uav_vehicle_control_grant
    WHERE tenant = $tenant AND work_context = $work_context
    AND principal_key = $principal AND session_id = $simulation_session AND vehicle_id = $vehicle
    AND revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)
    AND permissions CONTAINSALL $permissions
    AND ($profile = NONE OR map_mobility_profile_uri = $profile)
    AND ($advisory = false OR allow_planning_advisory = true)
    LIMIT 1
  )
) != 1 {
  THROW 'uav_execution_forbidden';
};
LET $observed = (
  SELECT *
  FROM ONLY $record
);
IF $observed != $expected_plan
OR $observed.state != 'prepared'
OR $observed.expires_at <= $now {
  THROW 'uav_execution_conflict';
};
UPDATE ONLY $record SET
  state = 'executing',
  canonical_json = $canonical,
  updated_at = $now,
  revision += 1
RETURN NONE;
CREATE ONLY $execution CONTENT {
  task: $task,
  plan: $record,
  tenant: $tenant,
  work_context: $work_context,
  principal_key: $principal,
  lease: $lease,
  lease_token: $lease_token,
  created_at: $now,
} RETURN NONE;
