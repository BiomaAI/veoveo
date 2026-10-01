BEGIN TRANSACTION;
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
    LET $pending = (
        SELECT VALUE id
        FROM knowledge_member
        WHERE
            generation = $generation
            AND collection = $collection
            AND stale = true
        LIMIT 1
    );
    IF array::len($pending) > 0 {
        THROW 'knowledge_collection_has_unresolved_members';
    };
    UPSERT $coverage CONTENT {
        generation: $generation,
        collection: $collection,
        revision: $revision,
    };
COMMIT TRANSACTION;
