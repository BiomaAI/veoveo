BEGIN TRANSACTION;
    LET $catalog = (
        SELECT *
        FROM ONLY $collection
        WHERE
            tenant = $tenant
            AND approved = true
            AND revision = $approval
    );
    LET $gen = (
        SELECT *
        FROM ONLY $generation
        WHERE
            tenant = $tenant
            AND state IN ['building', 'active']
            AND revision = $specification
    );
    IF $catalog = NONE
    OR $gen = NONE OR !(
        { record: $collection, revision: $approval } IN $gen.collections
    ) {
        THROW 'knowledge_read_not_admitted';
    };
    LET $old = (
        SELECT *
        FROM ONLY $member
    );
    UPSERT $member SET
        tenant = $tenant,
        generation = $generation,
        collection = $collection,
        uri = $uri,
        epoch = (
            $old.epoch ?? 0
        ) + 1,
        stale = true,
        deleted = false;
    DELETE $coverage;
    RETURN (
        SELECT VALUE epoch
        FROM ONLY $member
    );
COMMIT TRANSACTION;
