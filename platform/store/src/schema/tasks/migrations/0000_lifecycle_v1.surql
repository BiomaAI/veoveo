-- Trusted service metadata; this selection does not authorize a caller.
DEFINE FUNCTION fn::kernel::tasks::lifecycle_v1(
    $task: record<task>, $server: record<mcp_server>, $tenant: record<tenant>,
    $task_types: array<string>, $expected_result: option<object>
) -> option<object> {
    LET $rows = SELECT * FROM $task WHERE id = $task AND server = $server
        AND tenant = $tenant AND task_type IN $task_types;
    LET $row = array::first($rows);
    RETURN IF type::is_object($row) THEN {
        LET $result = $row.result;
        LET $matches = IF type::is_object($result) THEN {
            $expected_result != NONE AND $result.payload = $expected_result;
        } ELSE false END;
        {
            id: $row.id, server: $row.server, tenant: $row.tenant,
            owner: $row.owner, profile: $row.profile, work_context: $row.work_context,
            initiator: $row.initiator, task_type: $row.task_type, status: $row.status,
            created_at: $row.created_at, updated_at: $row.updated_at,
            completed_at: $row.completed_at, retention_expires_at: $row.retention_expires_at,
            retention_pins: $row.retention_pins, recovery_class: $row.recovery_class,
            result_matches: $matches
        };
    } ELSE NONE END;
} PERMISSIONS FULL;
