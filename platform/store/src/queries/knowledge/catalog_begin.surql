BEGIN TRANSACTION;
LET $head = SELECT * FROM ONLY gateway_control_active:current;
IF $head = NONE OR $head.revision.sha256 != $digest {
    THROW 'knowledge_control_revision_changed';
};
LET $prior = SELECT collection, revision FROM knowledge_collection
    WHERE tenant = $tenant ORDER BY collection LIMIT 1025;
RETURN {control: $head.revision, digest: $head.revision.sha256, prior: $prior};
COMMIT TRANSACTION;
