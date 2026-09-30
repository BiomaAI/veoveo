BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY audit_sealer:active FOR UPDATE);
IF $lease = NONE OR $lease.owner != $owner OR $lease.generation != $generation OR $lease.lease_until <= time::now() {
    THROW 'audit_lease_lost';
};
LET $block = (SELECT * FROM ONLY $block_id FOR UPDATE);
IF $block = NONE OR $block.block.head_hash != $block_hash { THROW 'audit_block_conflict'; };
LET $intent = (SELECT * FROM ONLY $intent_id FOR UPDATE);
IF $intent != NONE AND ($intent.block != $block_id OR $intent.destination != $destination OR $intent.payload != $payload) {
    THROW 'audit_block_conflict';
};
IF $intent != NONE AND $intent.rejection != NONE { THROW 'audit_export_rejected'; };
IF $intent = NONE OR $intent.delivered_at = NONE {
    LET $cursor = (SELECT * FROM ONLY $cursor_id FOR UPDATE);
    LET $anchor = (SELECT * FROM ONLY $anchor_id FOR UPDATE);
    IF $block.sequence != math::max([$cursor.sequence ?? 0, $anchor.sequence ?? 0]) + 1 {
        THROW 'audit_block_conflict';
    };
};
IF $rejection != NONE {
    IF $intent = NONE OR $intent.delivered_at != NONE { THROW 'audit_block_conflict'; };
    UPDATE $intent_id SET rejection = $rejection;
} ELSE IF $complete {
    IF $intent = NONE { THROW 'audit_block_conflict'; };
    IF $intent.delivered_at = NONE {
        UPDATE $intent_id SET delivered_at = time::now();
        UPDATE $block_id SET exported_to = array::union(exported_to, [$destination]);
        LET $cursor = (SELECT * FROM ONLY $cursor_id FOR UPDATE);
        IF $cursor != NONE AND $cursor.sequence >= $block.sequence { THROW 'audit_block_conflict'; };
        UPSERT $cursor_id SET sequence = $block.sequence, checkpoint = $checkpoint, payload = $payload, delivered_at = time::now();
    };
} ELSE IF $intent = NONE {
    CREATE $intent_id SET block = $block_id, destination = $destination, payload = $payload;
};
COMMIT TRANSACTION;
