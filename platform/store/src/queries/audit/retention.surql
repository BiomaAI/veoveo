BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY audit_sealer:active FOR UPDATE);
IF $lease = NONE OR $lease.owner != $owner OR $lease.generation != $generation OR $lease.lease_until <= time::now() {
    THROW 'audit_lease_lost';
};
LET $block = (SELECT * FROM ONLY $id FOR UPDATE);
LET $anchor = (SELECT * FROM ONLY $anchor_id FOR UPDATE);
IF $block = NONE OR $block.sealed_at >= $cutoff OR !($destinations ALLINSIDE $block.exported_to)
    OR ($anchor = NONE AND $block.sequence != 1)
    OR ($anchor != NONE AND $anchor.sequence + 1 != $block.sequence) {
    THROW 'audit_retention_not_admitted';
};
DELETE $block.records;
UPSERT $anchor_id SET partition = $block.partition, sequence = $block.sequence, block = $block.block;
DELETE $id;
COMMIT TRANSACTION;
