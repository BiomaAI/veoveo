LET $at = time::now();
LET $rows = SELECT id, updated_at, retention_expires_at,
    (SELECT metadata, labels, classification, work_context, authority, retention_expires_at, updated_at, blob
        FROM artifact_occurrence
        WHERE id = type::record('artifact_occurrence', <uuid> $parent.result.payload.structuredContent.results_artifact.artifact_id))[0] AS artifact,
    (SELECT id, out, permission, expires_at FROM artifact_grant
        WHERE in = type::record('artifact_occurrence', <uuid> $parent.result.payload.structuredContent.results_artifact.artifact_id)
        ORDER BY id) AS grants
FROM task
WHERE {{FINDING_ADMISSION}}
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
