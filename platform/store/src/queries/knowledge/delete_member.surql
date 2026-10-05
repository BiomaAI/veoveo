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
            AND revision = $approval
    );
    LET $gen = (
        SELECT *
        FROM ONLY $generation
        WHERE
            tenant = $tenant
            AND state IN ['building', 'active']
    );
    IF $sync.epoch != $source_epoch OR $sync.owner_epoch != $owner_epoch { THROW 'knowledge_source_changed'; };
    LET $slot = (
        SELECT *
        FROM ONLY $member
        WHERE
            tenant = $tenant
            AND generation = $generation
            AND collection = $collection
            AND epoch = $epoch
            AND stale = true
    );
    IF $catalog = NONE
    OR $gen = NONE
    OR $slot = NONE {
        THROW 'knowledge_deletion_fenced';
    };
    DELETE FROM type::table($chunk_table) WHERE member = $member;
    UPDATE ONLY $member SET
        stale = false,
        deleted = true,
        observation = NONE;
COMMIT TRANSACTION;
