SELECT identity.travel_model_id AS model_id, array::first(array::group({task: task, identity: identity, settlement: settlement})) AS record FROM map_travel_model_task WHERE identity.tenant = $tenant AND identity.profile = $profile_key
    AND (identity.tenant_key ?? NONE) = $tenant_key AND identity.data_labels ALLINSIDE $labels
    AND identity.authority_tenant = $authority_tenant
    AND fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile, $principal_key, $profile_key, $tenant_key, $labels, [$task_type], $context, $context_key, $authority_tenant) != NONE
    AND fn::kernel::tasks::input_matches_v1(task, identity.expected_input)
    AND settlement.status = 'succeeded' AND settlement.outcome = 'product' AND settlement.expected_result != NONE
    AND identity.created_by = $principal_key AND identity.work_context = $context_key
    AND {
        LET $lifecycle = fn::kernel::tasks::lifecycle_v1(task, $server, $tenant, [$task_type], settlement.expected_result);
        IF type::is_object($lifecycle) THEN $lifecycle.status = 'succeeded' AND $lifecycle.result_matches ELSE false END;
    } AND string::contains(string::lowercase(identity.travel_model_id), $needle) GROUP BY model_id ORDER BY model_id ASC LIMIT 101;
