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
) AND (true)
AND (created_at > $after_created_at OR (created_at = $after_created_at AND task > $after_task))
ORDER BY created_at ASC, task ASC LIMIT $limit;
