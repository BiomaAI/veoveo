SELECT VALUE external_job_id FROM provider_job WHERE tenant = $tenant AND task.tenant = $tenant
    AND task.server = $server AND task.owner = $owner AND task.profile = $profile
    AND task.request.owner.principal_key = $principal_key
    AND task.request.owner.profile = $profile_key
    AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
    AND task.request.owner.data_labels ALLINSIDE $labels AND provider = $provider AND external_job_id = provider_payload.id AND external_job_id > $after AND external_job_id != $after ORDER BY external_job_id ASC LIMIT $limit;
