BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

    LET $old = (
        SELECT *
        FROM ONLY $active
    );
    IF $old.generation != $previous {
        THROW 'knowledge_active_generation_changed';
    };
    LET $gen = (
        SELECT *
        FROM ONLY $generation
        WHERE
            tenant = $tenant
            AND state = 'building'
    );
    IF $gen = NONE {
        THROW 'knowledge_generation_not_building';
    };
    LET $approved = (
        SELECT VALUE id
        FROM knowledge_collection
        WHERE
            tenant = $tenant
            AND approved = true
    );
    IF array::len($approved) != array::len($gen.collections) {
        THROW 'knowledge_approval_set_changed';
    };
    -- Persisted requirement children are declared by the generation schema.
    FOR $required IN $gen.collections {
        LET $sync = type::record('knowledge_sync', [<string>record::id($generation), $required.record.collection]);
        IF $sync.ready != true OR $sync.owner_epoch != $owner_epoch { THROW 'knowledge_source_changed'; };
        LET $catalog = (
            SELECT *
            FROM ONLY $required.record
            WHERE
                tenant = $tenant
                AND approved = true
                AND revision = $required.revision
        );
        LET $coverage = (
            SELECT VALUE id
            FROM knowledge_coverage
            WHERE
                generation = $generation
                AND collection = $required.record
                AND revision = $required.revision
            LIMIT 1
        );
        IF $catalog = NONE
        OR array::len($coverage) != 1 {
            THROW 'knowledge_generation_incomplete';
        };
    };
    IF $previous != NONE {
        UPDATE ONLY $previous SET state = 'retired';
    };
    UPDATE ONLY $generation SET state = 'active';
    UPSERT $active CONTENT { generation: $generation };
COMMIT TRANSACTION;
