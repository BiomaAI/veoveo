SELECT VALUE identity.problem_id FROM (SELECT *, fn::kernel::tasks::selection_v1(
    task, $server, $tenant, $owner, $profile, $principal_key, $profile_key,
    $tenant_key, $labels, $task_types, $work_context, $work_context_key,
    $authority_tenant
) AS selected FROM optimization_task

)
WHERE (IF type::is_object(selected) {
    selected.task_type = task_type
} ELSE { false }
) AND (true)
    AND identity.problem_id CONTAINS $needle
GROUP BY identity.problem_id ORDER BY identity.problem_id ASC LIMIT $limit;
