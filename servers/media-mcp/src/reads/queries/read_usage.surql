SELECT IF (provider_job ?? NONE) = NONE THEN NONE ELSE receipt.association.prediction END AS provider_job_id, source_id, model_id, kind, quantity, unit, amount, currency, metadata, recorded_at, id FROM (
SELECT *,
    fn::kernel::tasks::lifecycle_v1(task, $server, tenant, ['run'], NONE) AS lifecycle,
    IF (provider_job ?? NONE) = NONE THEN NONE ELSE fn::kernel::tasks::provider_observation_v1(provider_job, task, $server, tenant, ['run'], $provider) END AS observation
FROM (
    SELECT *, array::first((SELECT * FROM media_task WHERE task = $parent.task LIMIT 1)) AS receipt
    FROM media_usage WHERE tenant = $tenant
)
)
WHERE type::is_object(receipt) AND receipt.identity.tenant = tenant
    AND type::is_object(lifecycle) AND lifecycle.task_type = receipt.task_type AND lifecycle.created_at = receipt.created_at
    AND fn::kernel::tasks::input_matches_v1(task, receipt.identity.request)
    AND fn::kernel::tasks::owner_context_matches_v1(task, receipt.identity.owner_context)
    AND type::is_object(fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile, $principal_key, $profile_key, $tenant_key, $labels, ['run'], NONE, NONE, NONE))
    AND ((provider_job ?? NONE) = NONE OR (
        type::is_object(receipt.association) AND provider_job = receipt.association.job
        AND receipt.association.provider = $provider
        AND type::is_object(observation) AND observation.external_job_id = receipt.association.prediction
    )) AND task = $task
ORDER BY recorded_at ASC, id ASC;
