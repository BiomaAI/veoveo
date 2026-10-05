LET $at = time::now();
LET $rows = SELECT id, updated_at, retention_expires_at,
    (SELECT metadata, labels, classification, work_context, authority, retention_expires_at, updated_at, blob
        FROM artifact_occurrence
        WHERE id = type::record('artifact_occurrence', <uuid> $parent.result.payload.structuredContent.results_artifact.artifact_id))[0] AS artifact,
    (SELECT id, out, permission, expires_at FROM artifact_grant
        WHERE in = type::record('artifact_occurrence', <uuid> $parent.result.payload.structuredContent.results_artifact.artifact_id)
        ORDER BY id) AS grants
FROM task
WHERE tenant = $tenant
    AND server = $reason_server
    AND task_type = $reason_task_type
    AND status = 'succeeded'
    AND (retention_expires_at = NONE OR retention_expires_at > time::now())
    AND (result.payload.isError ?? false) = false
    AND result.payload.structuredContent.results_artifact.artifact_id IN (
        SELECT VALUE type::string(record::id(id)) FROM artifact_occurrence
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

            AND metadata.provenance.kind = 'reason_results'
            AND metadata.provenance.analysis_id = type::string(record::id($parent.id))
    )

    AND ($analysis = NONE OR id = $analysis)
ORDER BY id
TIMEOUT 10s;
LET $deadline = array::min($rows.map(|$row| array::min([
    $row.retention_expires_at ?? time::maximum,
    $row.artifact.retention_expires_at ?? time::maximum,
    array::min($row.grants[WHERE expires_at > $at].expires_at) ?? time::maximum
])));
RETURN [[crypto::sha256(type::string($rows)), array::len($rows) > 0,
    IF $deadline = time::maximum { NONE } ELSE { $deadline }]];
