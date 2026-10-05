BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

    LET $catalog = (
        SELECT *
        FROM ONLY $collection
        WHERE
            tenant = $tenant
            AND approved = true
            AND revision = $revision
    );
    LET $gen = (
        SELECT *
        FROM ONLY $generation
        WHERE
            tenant = $tenant
            AND state IN ['building', 'active']
    );
    IF $catalog = NONE
    OR $gen = NONE OR !(
        { record: $collection, revision: $revision } IN $gen.collections
    ) {
        THROW 'knowledge_collection_changed';
    };
    IF $sync.epoch != $source_epoch OR $sync.owner_epoch != $owner_epoch OR $sync.coordinator != $coordinator { THROW 'knowledge_source_changed'; };
    LET $pending = (
        SELECT VALUE id
        FROM knowledge_member
        WHERE
            generation = $generation
            AND collection = $collection
            AND stale = true
            AND source_epoch = $sync.epoch
        LIMIT 1
    );
    IF array::len($pending) > 0 {
        THROW 'knowledge_collection_has_unresolved_members';
    };
    UPDATE ONLY $sync SET ready = true;
    UPSERT $coverage CONTENT {
        generation: $generation,
        collection: $collection,
        revision: $revision,
    };
COMMIT TRANSACTION;
