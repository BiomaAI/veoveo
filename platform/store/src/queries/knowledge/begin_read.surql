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
            AND revision = $specification
    );
    IF $catalog = NONE
    OR $gen = NONE OR !(
        { record: $collection, revision: $approval } IN $gen.collections
    ) {
        THROW 'knowledge_read_not_admitted';
    };
    LET $source = (SELECT * FROM ONLY $sync WHERE owner_epoch = $owner_epoch AND coordinator = $coordinator);
    IF $source = NONE { THROW 'knowledge_source_not_reconciled'; };
    LET $old = (
        SELECT *
        FROM ONLY $member
    );
    UPSERT $member SET
        tenant = $tenant,
        generation = $generation,
        collection = $collection,
        uri = $uri,
        sync = $sync,
        source_epoch = $source.epoch,
        epoch = (
            $old.epoch ?? 0
        ) + 1,
        stale = true,
        deleted = false;
    DELETE $coverage;
    RETURN {epoch: $member.epoch, source_epoch: $source.epoch, observation: $old.observation, title: $old.title};
COMMIT TRANSACTION;
