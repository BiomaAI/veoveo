SELECT task, task.result.payload.structuredContent AS result FROM provider_job
             WHERE tenant = $tenant AND task.tenant = $tenant
    AND task.server = $server AND task.owner = $owner AND task.profile = $profile
    AND task.owner_context.principal_key = $principal_key
    AND task.owner_context.profile = $profile_key
    AND (task.owner_context.tenant_key ?? NONE) = $tenant_key
    AND task.owner_context.data_labels ALLINSIDE $labels AND provider = $provider AND external_job_id = provider_payload.id AND external_job_id = $prediction
               AND task.status = 'succeeded' AND (task.result.payload.isError ?? false) = false
               AND task.result.payload.structuredContent.prediction.id = external_job_id LIMIT 1;
