BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY audit_sealer:active FOR UPDATE;
IF $current = NONE OR $current.owner != $owner OR $current.generation != $generation
    OR $current.lease_until <= time::now() { THROW 'audit_lease_lost'; };
UPDATE ONLY audit_sealer:active SET lease_until = time::now() + 30s,
    replayed_at = IF $caught_up { time::now() } ELSE { $current.replayed_at } RETURN AFTER;
COMMIT TRANSACTION;
