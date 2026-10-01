BEGIN TRANSACTION;
__LEASE__
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
    DELETE FROM __TABLE__ WHERE member = $member;
    UPDATE ONLY $member SET
        stale = false,
        deleted = true,
        observation = NONE;
COMMIT TRANSACTION;
