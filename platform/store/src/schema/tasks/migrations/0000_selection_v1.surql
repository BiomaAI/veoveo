-- Caller scope is service-admitted. This function reads persisted Task policy, not a caller object.
DEFINE FUNCTION fn::kernel::tasks::selection_v1(
    $task: record<task>,
    $server: record<mcp_server>,
    $tenant: record<tenant>,
    $owner: record<principal>,
    $profile: record<profile>,
    $principal_key: string,
    $profile_key: string,
    $tenant_key: option<string>,
    $labels: array<string>,
    $task_types: option<array<string>>,
    $work_context: option<record<work_context>>,
    $work_context_key: option<string>,
    $authority_tenant: option<string>
) -> option<object> {
    LET $rows = SELECT * FROM task WHERE id = $task
        AND server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile
        AND ($task_types = NONE OR task_type IN $task_types) LIMIT 1;
    LET $row = array::first($rows);
    RETURN IF type::is_object($row) THEN {
        LET $request = $row.request;
        IF type::is_object($request) THEN {
            LET $request_owner = $request.owner;
            IF type::is_object($request_owner) THEN {
                LET $labels_agree = IF type::is_array($request_owner.data_labels) THEN $request_owner.data_labels ALLINSIDE $labels ELSE false END;
                LET $owner_agrees = $request_owner.principal_key = $principal_key
                    AND $request_owner.profile = $profile_key
                    AND ($request_owner.tenant_key ?? NONE) = $tenant_key
                    AND $labels_agree;
                LET $context_agrees = IF $work_context = NONE THEN {
                    $work_context_key = NONE AND $authority_tenant = NONE;
                } ELSE {
                    LET $authority = $row.authority;
                    IF type::is_object($authority) THEN {
                        LET $request_authority = $request_owner.authority;
                        IF type::is_object($request_authority) THEN {
                            $work_context_key != NONE AND $authority_tenant != NONE
                                AND $row.work_context = $work_context
                                AND $authority.context_key = $work_context_key
                                AND $request_authority.work_context = $work_context_key
                                AND $request_authority.tenant = $authority_tenant;
                        } ELSE false END;
                    } ELSE false END;
                } END;
                LET $result = $row.result;
                LET $result_payload = IF type::is_object($result) THEN {
                    $result.payload;
                } ELSE NONE END;
                IF $owner_agrees AND $context_agrees THEN {
                    {
                        id: $row.id, task_type: $row.task_type, status: $row.status,
                        created_at: $row.created_at, input: $request.input, result: $result_payload
                    };
                } ELSE NONE END;
            } ELSE NONE END;
        } ELSE NONE END;
    } ELSE NONE END;
} PERMISSIONS FULL;
