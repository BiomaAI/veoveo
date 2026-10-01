tenant = $tenant
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
