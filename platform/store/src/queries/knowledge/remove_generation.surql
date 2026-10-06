BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

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
    -- Remove row references while the declared producer_batch field still exists.
    DELETE type::table($chunk_table);
    REMOVE TABLE $chunk_table;
    DELETE $generation;
COMMIT TRANSACTION;
