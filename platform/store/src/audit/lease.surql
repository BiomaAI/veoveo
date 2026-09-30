BEGIN TRANSACTION;
LET $current = (SELECT * FROM ONLY audit_sealer:active FOR UPDATE);
IF $current != NONE AND $current.owner != $owner AND $current.lease_until > time::now() {
    THROW 'audit_lease_busy';
};
IF $current != NONE AND $current.replayed_at < time::now() - 6d {
    THROW 'audit_changefeed_gap';
};
LET $generation = IF $current = NONE { 1 } ELSE IF $current.owner = $owner AND $current.lease_until > time::now() { $current.generation } ELSE { $current.generation + 1 };
LET $cursor = IF $current = NONE { 0 } ELSE { $current.cursor };
LET $replayed_at = IF $current = NONE { time::now() } ELSE { $current.replayed_at };
UPSERT ONLY audit_sealer:active SET owner = $owner, generation = $generation,
    lease_until = time::now() + 30s, cursor = $cursor, replayed_at = $replayed_at RETURN AFTER;
COMMIT TRANSACTION;
