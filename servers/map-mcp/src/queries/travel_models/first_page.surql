SELECT record::id(id) AS task_id, result.payload.structuredContent AS record FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND work_context = $context
    AND owner_context.principal_key = $principal_key AND owner_context.profile = $profile_key
    AND (owner_context.tenant_key ?? NONE) = $tenant_key
    AND owner_context.data_labels ALLINSIDE $labels
    AND authority.context_key = $context_key
    AND owner_context.authority.work_context = $context_key
    AND owner_context.authority.tenant = $authority_tenant
    AND request.input.request.identity.actor.id = $principal_key
    AND request.input.request.identity.profile = $profile_key
    AND (request.input.request.identity.actor.tenant ?? NONE) = $tenant_key
    AND request.input.request.identity.actor.data_labels ALLINSIDE $labels
    AND request.input.request.identity.authority.work_context = $context_key
    AND request.input.request.identity.authority.tenant = $authority_tenant AND task_type = $task_type
    AND request.input.kind = $task_type
    AND status = 'succeeded' AND (result.payload.isError ?? false) = false
    AND result.payload.structuredContent.created_by = $principal_key
    AND result.payload.structuredContent.work_context = $context_key
    AND result.payload.structuredContent.travel_model_id = request.input.request.travel_model_id  ORDER BY task_id ASC LIMIT 101;
