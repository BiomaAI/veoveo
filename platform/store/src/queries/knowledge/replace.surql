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
    -- VALIDATE_EMBEDDING_RUNTIME
    DELETE FROM type::table($chunk_table) WHERE member = $member;
    CREATE ONLY $embedding_batch CONTENT {
        generation: $generation,
        member_uri: $member.uri,
        collection: $collection,
        source_epoch: $source_epoch,
        member_epoch: $epoch,
        producer_profile: $embedding_producer,
        chunk_count: array::len($chunks),
        recorded_at: time::now()
    };
    INSERT INTO $chunk_target $chunks RETURN NONE;
    UPDATE ONLY $generation SET
        producer_profiles = array::distinct(array::append(producer_profiles, $embedding_producer)),
        profile_epoch = $gen.profile_epoch + IF $embedding_producer IN $gen.producer_profiles { 0 } ELSE { 1 };
    UPDATE ONLY $member SET
        stale = false,
        deleted = false,
        observation = $observation,
        title = $title,
        valid_until = $valid_until;
COMMIT TRANSACTION;
