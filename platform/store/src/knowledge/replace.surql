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
            AND revision = $specification
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
        THROW 'knowledge_read_fenced';
    };
    DELETE FROM __TABLE__ WHERE member = $member;
    INSERT INTO __TABLE__ $chunks RETURN NONE;
    UPDATE ONLY $member SET
        stale = false,
        deleted = false,
        observation = $observation,
        title = $title,
        valid_until = $valid_until;
COMMIT TRANSACTION;
