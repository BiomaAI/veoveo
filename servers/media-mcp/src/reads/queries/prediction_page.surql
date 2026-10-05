SELECT VALUE external_job_id FROM provider_job WHERE tenant = $tenant AND task.tenant = $tenant
    AND task.server = $server AND task.owner = $owner AND task.profile = $profile
    AND task.owner_context.principal_key = $principal_key
    AND task.owner_context.profile = $profile_key
    AND (task.owner_context.tenant_key ?? NONE) = $tenant_key
    AND task.owner_context.data_labels ALLINSIDE $labels AND provider = $provider AND external_job_id = provider_payload.id  ORDER BY external_job_id ASC LIMIT $limit;
