SELECT * FROM (
SELECT association.job AS id, association.job AS job_order, task, observation.external_job_id AS external_job_id,
    observation.state AS state, observation.provider_payload AS provider_payload,
    observation.updated_at AS updated_at
FROM (
    SELECT task, task_type, created_at, identity, association,
        fn::kernel::tasks::lifecycle_v1(task, mcp_server:media, identity.tenant, ['run'], NONE) AS lifecycle,
        fn::kernel::tasks::provider_observation_v1(association.job, task, mcp_server:media, identity.tenant, ['run'], $provider) AS observation
    FROM media_task WHERE type::is_object(association) AND association.provider = $provider
)
WHERE type::is_object(lifecycle) AND lifecycle.task_type = task_type AND lifecycle.created_at = created_at
    AND fn::kernel::tasks::input_matches_v1(task, identity.request)
    AND fn::kernel::tasks::owner_context_matches_v1(task, identity.owner_context)
    AND type::is_object(observation) AND observation.external_job_id = association.prediction
    AND observation.terminal_event != NONE AND observation.observed_at != NONE
    AND ($provider_retention_pin IN lifecycle.retention_pins OR (SELECT VALUE id FROM media_usage
        WHERE task = $parent.task AND tenant = $parent.identity.tenant
            AND provider_job = $parent.association.job AND kind = 'actual' LIMIT 1) = [])
) ORDER BY job_order ASC LIMIT 101;
