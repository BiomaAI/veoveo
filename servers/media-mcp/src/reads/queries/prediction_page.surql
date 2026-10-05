SELECT VALUE association.prediction FROM (
SELECT task, task_type, created_at, identity, association, settlement,
    fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile, $principal_key, $profile_key, $tenant_key, $labels, ['run'], NONE, NONE, NONE) AS admitted,
    fn::kernel::tasks::lifecycle_v1(task, $server, identity.tenant, ['run'], settlement.expected_result) AS lifecycle,
    fn::kernel::tasks::provider_observation_v1(association.job, task, $server, identity.tenant, ['run'], $provider) AS observation
FROM media_task
WHERE identity.tenant = $tenant AND type::is_object(association) AND association.provider = $provider
)
WHERE type::is_object(admitted) AND admitted.task_type = task_type AND admitted.created_at = created_at
    AND fn::kernel::tasks::input_matches_v1(task, identity.request)
    AND fn::kernel::tasks::owner_context_matches_v1(task, identity.owner_context)
    AND type::is_object(observation) AND observation.external_job_id = association.prediction
ORDER BY association.prediction ASC LIMIT $limit;
