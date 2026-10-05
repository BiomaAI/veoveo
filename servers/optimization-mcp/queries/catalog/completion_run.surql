SELECT VALUE identity.run_id FROM (SELECT *, fn::kernel::tasks::selection_v1(
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
    AND identity.run_id CONTAINS $needle
GROUP BY identity.run_id ORDER BY identity.run_id ASC LIMIT $limit;
