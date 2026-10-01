BEGIN TRANSACTION;
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
    FOR $required IN $gen.collections {
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
