BEGIN TRANSACTION;
IF $task != NONE {
    LET $admitted = SELECT VALUE id FROM task WHERE id = $task AND server = mcp_server:frames
        AND tenant = $tenant AND owner = $owner AND profile = $profile
        AND request.owner.principal_key = $principal_key
        AND request.owner.profile = $profile_key
        AND (request.owner.tenant_key ?? NONE) = $tenant_key
        AND request.owner.data_labels ALLINSIDE $clearance LIMIT 1;
    IF array::len($admitted) != 1 { THROW 'Frames operation Task authority does not match'; };
};
LET $existing = SELECT * FROM ONLY $operation;
IF $existing != NONE {
    IF $existing.tenant != $content.tenant OR $existing.owner != $content.owner
        OR $existing.task != $content.task OR $existing.authority != $content.authority
        OR $existing.operation_key != $content.operation_key OR $existing.kind != $content.kind
        OR $existing.provenance != $content.provenance OR $existing.classification != $content.classification
        OR $existing.labels != $content.labels OR $existing.created_at != $content.created_at {
        THROW 'Frames operation conflicts with recorded authority or provenance';
    };
} ELSE {
    CREATE ONLY $operation CONTENT $content RETURN NONE;
    };
COMMIT TRANSACTION;
