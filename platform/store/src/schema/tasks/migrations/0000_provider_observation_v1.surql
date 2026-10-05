-- Whole opaque observation facts; caller authorization is a separate selection_v1 admission.
DEFINE FUNCTION fn::kernel::tasks::provider_observation_v1(
    $job: record<provider_job>, $task: record<task>, $server: record<mcp_server>,
    $tenant: record<tenant>, $task_types: array<string>, $provider: string
) -> option<object> {
    LET $tasks = SELECT * FROM $task WHERE id = $task AND server = $server
        AND tenant = $tenant AND task_type IN $task_types AND recovery_class = 'webhook_wait';
    LET $life = array::first($tasks);
    LET $rows = SELECT * FROM $job WHERE id = $job AND task = $task AND tenant = $tenant AND provider = $provider;
    LET $row = array::first($rows);
    RETURN IF type::is_object($life) THEN {
      IF type::is_object($row) THEN {
        LET $receipt_agrees = IF $row.terminal_event = NONE THEN $row.observed_at = NONE ELSE {
            LET $terminal: record<provider_event> = $row.terminal_event;
            LET $events = SELECT * FROM $terminal WHERE id = $terminal
                AND provider_job = $job AND tenant = $tenant AND provider = $provider
                AND payload = $row.provider_payload;
            array::len($events) = 1 AND $row.observed_at != NONE;
        } END;
        IF $receipt_agrees THEN {
            {
                external_job_id: $row.external_job_id,
                state: $row.state,
                provider_payload: $row.provider_payload,
                terminal_event: $row.terminal_event,
                observed_at: $row.observed_at,
                updated_at: $row.updated_at
            };
        } ELSE NONE END;
      } ELSE NONE END;
    } ELSE NONE END;
} PERMISSIONS FULL;
