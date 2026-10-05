RETURN count((SELECT VALUE id FROM (
    SELECT id, task, tenant, provider_job,
        array::first((SELECT task, task_type, created_at, identity, association,
            fn::kernel::tasks::lifecycle_v1(task, mcp_server:media, identity.tenant, ['run'], NONE) AS lifecycle,
            fn::kernel::tasks::provider_observation_v1(association.job, task, mcp_server:media, identity.tenant, ['run'], $provider) AS observation
            FROM media_task WHERE task = $parent.task AND type::is_object(association)
                AND association.provider = $provider AND association.prediction = $prediction LIMIT 1)) AS receipt
    FROM media_usage WHERE task = $task AND kind = 'actual'
)
WHERE type::is_object(receipt) AND receipt.identity.tenant = tenant AND receipt.association.job = provider_job
    AND type::is_object(receipt.lifecycle) AND receipt.lifecycle.task_type = receipt.task_type
    AND receipt.lifecycle.created_at = receipt.created_at
    AND fn::kernel::tasks::input_matches_v1(task, receipt.identity.request)
    AND fn::kernel::tasks::owner_context_matches_v1(task, receipt.identity.owner_context)
    AND type::is_object(receipt.observation) AND receipt.observation.external_job_id = $prediction
LIMIT 1)) > 0;
