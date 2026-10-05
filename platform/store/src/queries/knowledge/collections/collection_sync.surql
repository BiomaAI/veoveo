BEGIN TRANSACTION; LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;
 RETURN (SELECT VALUE epoch FROM ONLY $sync WHERE coordinator = $coordinator AND owner_epoch = $owner_epoch AND collection.revision = $approval AND collection.approved = true AND generation.state IN ['building', 'active']); COMMIT TRANSACTION;
