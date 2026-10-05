SELECT * FROM (SELECT *, fn::kernel::tasks::selection_v1(
    task, $server, $tenant, $owner, $profile, $principal_key, $profile_key,
    $tenant_key, $labels, $task_types, $work_context, $work_context_key,
    $authority_tenant
) AS selected FROM optimization_task

)
WHERE (IF type::is_object(selected) {
    IF type::is_object(selected.input) {
        selected.input.kind = selected.task_type
    } ELSE { false }
} ELSE { false }
) AND (IF type::is_object(selected) {
    IF selected.status = 'succeeded' AND type::is_object(selected.result) {
        IF (selected.result.isError ?? false) = false AND type::is_object(selected.result.structuredContent) {
            selected.result.structuredContent.result_uri != NONE
        } ELSE { false }
    } ELSE { false }
} ELSE { false }
)

ORDER BY created_at ASC, task ASC LIMIT $limit;
