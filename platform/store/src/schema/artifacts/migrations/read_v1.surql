-- Pure admission: outer queries obtain one database timestamp and bind it here.
DEFINE FUNCTION fn::kernel::artifacts::read_v1(
    $artifact: record<artifact_occurrence>, $tenant: record<tenant>,
    $principals: array<record<principal>>, $groups: array<record<principal_group>>,
    $clearance: array<string>, $context: option<record<work_context>>,
    $context_key: option<string>, $at: datetime
) -> option<object> {
    LET $rows = SELECT * FROM artifact_occurrence WHERE id = $artifact AND tenant = $tenant
        AND labels ALLINSIDE $clearance
        AND (classification = '' OR classification IN $clearance)
        AND (retention_expires_at = NONE OR retention_expires_at > $at) LIMIT 1;
    LET $row = array::first($rows);
    RETURN IF type::is_object($row) THEN {
        LET $active = SELECT id FROM artifact_grant WHERE in = $artifact
            AND (out IN $principals OR out IN $groups)
            AND permission IN ['read', 'write', 'admin']
            AND (expires_at = NONE OR expires_at > $at) LIMIT 1;
        LET $authority = $row.authority;
        LET $context_admitted = IF type::is_object($authority) THEN {
            $context != NONE AND $row.work_context = $context AND $authority.context_key = $context_key;
        } ELSE false END;
        IF $context_admitted OR array::len($active) > 0 THEN {
            LET $grants = SELECT id, out, permission, expires_at FROM artifact_grant
                WHERE in = $artifact ORDER BY id;
            LET $future = SELECT VALUE expires_at FROM artifact_grant
                WHERE in = $artifact AND expires_at > $at ORDER BY expires_at LIMIT 1;
            LET $deadline = array::min([$row.retention_expires_at ?? time::maximum,
                array::min($future) ?? time::maximum]);
            { id: $row.id, metadata: $row.metadata,
                observation: { metadata: $row.metadata, labels: $row.labels,
                    classification: $row.classification, work_context: $row.work_context,
                    authority: $authority, retention_expires_at: $row.retention_expires_at,
                    updated_at: $row.updated_at, blob: $row.blob, grants: $grants },
                deadline: IF $deadline = time::maximum THEN NONE ELSE $deadline END };
        } ELSE NONE END;
    } ELSE NONE END;
} PERMISSIONS FULL;
