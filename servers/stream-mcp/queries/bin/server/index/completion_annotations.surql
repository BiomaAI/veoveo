SELECT VALUE candidate FROM (SELECT type::string(record::id(settlement.annotations)) AS candidate FROM stream_run
WHERE task_type = $task_type AND settlement.annotations != NONE AND fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile, $principal_key, $profile_key, $tenant_key, $data_labels, [$task_type], NONE, NONE, NONE) != NONE
    AND type::string(record::id(settlement.annotations)) CONTAINS $needle
GROUP BY candidate ORDER BY candidate ASC LIMIT $limit);
