BEGIN TRANSACTION;
__LEASE__
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
