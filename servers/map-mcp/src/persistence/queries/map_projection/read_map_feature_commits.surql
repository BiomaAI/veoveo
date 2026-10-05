SELECT IF (fn::kernel::identity::tenant_context_keys_v1(tenant, work_context)).context_key = work_context_key THEN (fn::kernel::identity::tenant_context_keys_v1(tenant, work_context)).tenant_key ELSE NONE END AS tenant_key,
    work_context_key, layer_key, changeset_key, commit_sequence, resulting_layer_revision, feature_keys
FROM map_feature_changeset
WHERE commit_sequence > $after AND commit_sequence <= $through
ORDER BY commit_sequence ASC LIMIT $limit;
