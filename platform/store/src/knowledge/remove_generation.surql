BEGIN TRANSACTION;
    LET $gen = (
        SELECT *
        FROM ONLY $generation
        WHERE
            tenant = $tenant
            AND state IN ['building', 'retired']
    );
    IF $gen = NONE
    OR $active.generation = $generation {
        THROW 'knowledge_generation_not_reclaimable';
    };
    DELETE $generation;
    REMOVE TABLE __TABLE__;
COMMIT TRANSACTION;
