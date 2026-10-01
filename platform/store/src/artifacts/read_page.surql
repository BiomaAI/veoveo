RETURN (SELECT * FROM artifact_occurrence
WHERE tenant = $tenant
    AND ($artifact = NONE OR id = $artifact)
    AND labels ALLINSIDE $clearance
    AND (classification = '' OR classification IN $clearance)
    AND (retention_expires_at = NONE OR retention_expires_at > time::now())
    AND (
        ($context != NONE AND work_context = $context AND authority.context_key = $context_key)
        OR id IN (
            SELECT VALUE in FROM artifact_grant
            WHERE out IN $subjects
                AND permission IN ['read', 'write', 'admin']
                AND (expires_at = NONE OR expires_at > time::now())
        )
    )
    AND ($cursor = NONE OR id < $cursor)
ORDER BY id DESC
LIMIT $limit).map(|$occurrence| {
    occurrence: $occurrence,
    blob: (SELECT * FROM ONLY $occurrence.blob),
    tenant: (SELECT * FROM ONLY $occurrence.tenant),
    grants: (SELECT * FROM artifact_grant WHERE in = $occurrence.id ORDER BY id)
});
