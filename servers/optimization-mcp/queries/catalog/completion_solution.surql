SELECT VALUE settlement.result_uri FROM (SELECT *, fn::kernel::tasks::selection_v1(
    task, $server, $tenant, $owner, $profile, $principal_key, $profile_key,
    $tenant_key, $labels, $task_types, $work_context, $work_context_key,
    $authority_tenant
) AS selected FROM optimization_task

)
WHERE (IF type::is_object(selected) {
    selected.task_type = task_type
} ELSE { false }
) AND (IF type::is_object(selected) {
    IF selected.status = 'succeeded' AND type::is_object(settlement) {
        settlement.status = 'succeeded' AND settlement.result_uri != NONE
    } ELSE { false }
} ELSE { false }
)
    AND settlement.result_uri CONTAINS $needle
GROUP BY settlement.result_uri ORDER BY settlement.result_uri ASC LIMIT $limit;
