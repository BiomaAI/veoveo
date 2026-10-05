LET $at = time::now();
SELECT id, task, task_type, created_at, identity, settlement,
    fn::kernel::tasks::lifecycle_v1(task, $reason_server, $tenant, [$reason_task_type], settlement.expected_result) AS lifecycle
FROM reason_analysis
WHERE identity.tenant = $tenant AND task_type = $reason_task_type AND settlement.status = 'succeeded' AND settlement.outcome = 'product'
    AND ($analysis = NONE OR task = $analysis)
    AND ($after_id = NONE OR created_at < $after_created_at OR (created_at = $after_created_at AND task < $after_id))
    AND ($prefix = NONE OR string::starts_with(type::string(record::id(task)), $prefix))
    AND {
        LET $lifecycle = fn::kernel::tasks::lifecycle_v1(task, $reason_server, $tenant, [$reason_task_type], settlement.expected_result);
        IF type::is_object($lifecycle) THEN {
            IF $lifecycle.status = 'succeeded' AND ($lifecycle.retention_expires_at = NONE OR $lifecycle.retention_expires_at > $at) THEN {
                LET $artifact = fn::kernel::artifacts::read_v1(settlement.results, $tenant, $principals, $groups, $clearance, $context, $context_key, $at);
                IF type::is_object($artifact) THEN $artifact.metadata = settlement.expected_metadata ELSE false END;
            } ELSE false END;
        } ELSE false END;
    }
ORDER BY created_at DESC, task DESC LIMIT $limit TIMEOUT 10s;
