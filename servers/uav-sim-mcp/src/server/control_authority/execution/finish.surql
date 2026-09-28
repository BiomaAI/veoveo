BEGIN TRANSACTION;
LET $current_lease = (SELECT * FROM ONLY $lease
    WHERE tenant = $tenant AND work_context = $work_context
        AND principal_key = $principal AND session_id = $simulation_session
        AND vehicle_id = $vehicle AND mission_id = $mission AND lease_token = $lease_token);
IF $current_lease = NONE { THROW 'uav_execution_conflict'; };
LET $observed = (SELECT * FROM ONLY $record);
IF $observed != $expected_plan { THROW 'uav_execution_conflict'; };
IF $observed.state = 'executing' {
    IF $current_lease.released_at != NONE OR $current_lease.revision >= $max_revision {
        THROW 'uav_execution_conflict';
    };
    UPDATE ONLY $lease SET released_at = $now, updated_at = $now, revision += 1 RETURN NONE;
    UPDATE ONLY $record SET state = $state, canonical_json = $canonical,
        updated_at = $now, revision += 1 RETURN NONE;
} ELSE IF $observed.state != $state OR $current_lease.released_at = NONE {
    THROW 'uav_execution_conflict';
};
COMMIT TRANSACTION;
