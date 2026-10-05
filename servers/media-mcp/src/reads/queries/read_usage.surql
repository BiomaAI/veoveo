SELECT provider_job.external_job_id AS provider_job_id, source_id, model_id, kind, quantity, unit, amount, currency, metadata, recorded_at, id FROM media_usage WHERE tenant = $tenant AND task.tenant = $tenant
    AND task.server = $server AND task.owner = $owner AND task.profile = $profile
    AND task.request.owner.principal_key = $principal_key
    AND task.request.owner.profile = $profile_key
    AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
    AND task.request.owner.data_labels ALLINSIDE $labels AND ((provider_job ?? NONE) = NONE OR (
    provider_job.tenant = $tenant AND provider_job.task = task
    AND provider_job.provider = $provider
    AND provider_job.external_job_id = provider_job.provider_payload.id)) AND task = $task ORDER BY recorded_at ASC, id ASC;
