BEGIN TRANSACTION;
IF $task != NONE {
    LET $admitted = fn::kernel::tasks::selection_v1(
        $task, mcp_server:frames, $tenant, $owner, $profile,
        $principal_key, $profile_key, $tenant_key, $clearance,
        NONE, NONE, NONE, NONE);
    IF $admitted = NONE { THROW 'Frames operation Task authority does not match'; };
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
