SELECT * FROM (SELECT *, fn::kernel::tasks::selection_v1(
    task, $server, $tenant, $owner, $profile, $principal_key, $profile_key,
    $tenant_key, $labels, $task_types, $work_context, $work_context_key,
    $authority_tenant
) AS selected FROM optimization_task

)
WHERE (IF type::is_object(selected) {
    selected.task_type = task_type
} ELSE { false }
) AND (true)

ORDER BY created_at ASC, task ASC LIMIT $limit;
