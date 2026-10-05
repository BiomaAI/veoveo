BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

LET $catalog = (SELECT * FROM ONLY $collection WHERE tenant = $tenant AND revision = $approval AND approved = true);
LET $gen = (SELECT * FROM ONLY $generation WHERE tenant = $tenant AND state IN ['building', 'active']);
IF $catalog = NONE OR $gen = NONE OR !({record: $collection, revision: $approval} IN $gen.collections) { THROW 'knowledge_collection_changed'; };
LET $prior = (SELECT * FROM ONLY $sync);
UPSERT $sync SET generation = $generation, collection = $collection, coordinator = $coordinator,
    owner_epoch = $owner_epoch, epoch = ($prior.epoch ?? 0) + 1, ready = false, changed_at = time::now();
DELETE $coverage;
RETURN $sync.epoch;
COMMIT TRANSACTION;
