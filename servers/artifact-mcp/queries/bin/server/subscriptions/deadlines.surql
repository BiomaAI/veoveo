LET $at = time::now();
LET $rows = SELECT retention_expires_at,
    (SELECT VALUE expires_at FROM artifact_grant
        WHERE in = $parent.id AND expires_at > $at) AS deadlines
FROM artifact_occurrence
WHERE tenant = $tenant
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
 AND ($all OR id IN $members)
TIMEOUT 10s;
LET $deadline = array::min($rows.map(|$row| array::min([
    $row.retention_expires_at ?? time::maximum,
    array::min($row.deadlines) ?? time::maximum
])));
RETURN [IF $deadline = time::maximum { NONE } ELSE { $deadline }];
