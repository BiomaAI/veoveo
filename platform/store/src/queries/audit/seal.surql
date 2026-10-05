BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY audit_sealer:active FOR UPDATE);
IF $lease = NONE OR $lease.owner != $owner OR $lease.generation != $generation
    OR $lease.lease_until <= time::now() OR $lease.cursor != $expected_cursor {
    THROW 'audit_lease_lost';
};
FOR $item IN $blocks {
    LET $head_id = $item.head_id;
    LET $head = (SELECT * FROM ONLY $head_id FOR UPDATE);
    IF ($head = NONE AND $item.sequence != 1)
        OR ($head != NONE AND ($head.sequence + 1 != $item.sequence OR $head.head_hash != $item.previous)) {
        THROW 'audit_block_conflict';
    };
    CREATE $item.id CONTENT $item.row;
    FOR $member IN $item.row.records {
        LET $membership = type::record('audit_record_seal', record::id($member));
        IF record::exists($membership) { THROW 'audit_block_conflict'; };
        CREATE $membership SET record = $member, block = $item.id;
    };
    UPSERT $item.head_id SET partition = $item.partition, sequence = $item.sequence,
        head_hash = $item.head_hash, checkpoint = $item.checkpoint;
};
UPDATE ONLY audit_sealer:active SET cursor = $next_cursor, replayed_at = time::now(), lease_until = time::now() + 30s;
COMMIT TRANSACTION;
