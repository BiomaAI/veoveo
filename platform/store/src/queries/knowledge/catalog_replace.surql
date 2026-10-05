BEGIN TRANSACTION;
LET $head = SELECT * FROM ONLY gateway_control_active:current FOR UPDATE;
IF $head = NONE OR $head.revision != $control OR $head.revision.sha256 != $digest {
    THROW 'knowledge_control_revision_changed';
};
LET $current = SELECT collection, revision FROM knowledge_collection
    WHERE tenant = $tenant ORDER BY collection LIMIT 1025;
IF $current != $prior { THROW 'knowledge_catalog_revision_changed'; };
DELETE knowledge_collection WHERE tenant = $tenant AND collection NOT IN $selected;
FOR $row IN $registrations {
    UPSERT $row.id CONTENT $row;
};
COMMIT TRANSACTION;
