BEGIN TRANSACTION;
__LEASE__
LET $catalog = (SELECT * FROM ONLY $collection WHERE tenant = $tenant AND revision = $approval AND approved = true);
LET $gen = (SELECT * FROM ONLY $generation WHERE tenant = $tenant AND state IN ['building', 'active']);
IF $catalog = NONE OR $gen = NONE OR !({record: $collection, revision: $approval} IN $gen.collections) { THROW 'knowledge_collection_changed'; };
LET $prior = (SELECT * FROM ONLY $sync);
UPSERT $sync SET generation = $generation, collection = $collection, coordinator = $coordinator,
    owner_epoch = $owner_epoch, epoch = ($prior.epoch ?? 0) + 1, ready = false, changed_at = time::now();
DELETE $coverage;
RETURN $sync.epoch;
COMMIT TRANSACTION;
