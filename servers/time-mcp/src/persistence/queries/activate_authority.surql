BEGIN TRANSACTION;
-- Lock both exact pointer IDs, even when one or both records are absent.
-- Callers bind pointer and release IDs in stable order before snapshot comparison.
FOR $pointer IN $pointers {
    LET $locked_pointer = (SELECT * FROM ONLY $pointer FOR UPDATE);
};
FOR $release_to_lock IN $releases {
    LET $locked_release = (SELECT * FROM ONLY $release_to_lock FOR UPDATE);
};
LET $observed = {
SELECT dataset_kind, $this AS pointer,
    (SELECT * FROM type::record('time_authority_release', $parent.release_key)
        WHERE tenant = $tenant AND dataset_kind = $parent.dataset_kind
        AND release_key = $parent.release_key AND state = 'active')[0] AS release
FROM time_active_authority
WHERE tenant = $tenant AND ($kind = NONE OR dataset_kind = $kind)
ORDER BY dataset_kind ASC;
};
IF array::len($observed) != array::len($expected_authorities) {
    THROW 'time_authority_pair_conflict';
};
FOR $index IN 0..array::len($observed) {
    LET $current = $observed[$index];
    LET $expected = $expected_authorities[$index];
    -- Optional history is absent in stored objects and NONE in driver values.
    -- Compare its value, and every other pointer field, without object-key ambiguity.
    IF $current.dataset_kind != $expected.dataset_kind
        OR $current.release != $expected.release
        OR $current.pointer.id != $expected.pointer.id
        OR $current.pointer.tenant != $expected.pointer.tenant
        OR $current.pointer.dataset_kind != $expected.pointer.dataset_kind
        OR $current.pointer.release_key != $expected.pointer.release_key
        OR $current.pointer.previous_release_key != $expected.pointer.previous_release_key
        OR $current.pointer.activated_by != $expected.pointer.activated_by
        OR $current.pointer.activated_at != $expected.pointer.activated_at
        OR $current.pointer.record_version != $expected.pointer.record_version {
        THROW 'time_authority_pair_conflict';
    };
};
LET $candidate = (SELECT * FROM ONLY $release WHERE tenant = $tenant);
IF $candidate != $expected_candidate { THROW 'time_authority_candidate_conflict'; };
LET $release_updated = (UPDATE ONLY $release MERGE {
    state: 'active', canonical_json: $canonical_json,
    record_version: $next_release, updated_at: time::now()
} WHERE tenant = $tenant AND dataset_kind = $dataset_kind
    AND release_key = $release_key AND state = 'staged'
    AND record_version = $expected_release RETURN AFTER);
IF $release_updated = NONE { THROW 'time_authority_release_conflict'; };
IF $expected_pointer = 0 {
    CREATE ONLY $active CONTENT {
        tenant: $tenant, dataset_kind: $dataset_kind, release_key: $release_key,
        previous_release_key: NONE, activated_by: $owner,
        activated_at: time::now(), record_version: 1
    } RETURN NONE;
} ELSE {
    LET $pointer_updated = (UPDATE ONLY $active MERGE {
        release_key: $release_key, previous_release_key: $previous,
        activated_by: $owner, activated_at: time::now(), record_version: $next_pointer
    } WHERE tenant = $tenant AND dataset_kind = $dataset_kind
        AND release_key = $previous AND previous_release_key = $expected_previous
        AND record_version = $expected_pointer RETURN AFTER);
    IF $pointer_updated = NONE { THROW 'time_active_authority_conflict'; };
    LET $previous_retired = (UPDATE ONLY $previous_release MERGE {
        state: 'retired', record_version: record_version + 1, updated_at: time::now()
    } WHERE tenant = $tenant AND dataset_kind = $dataset_kind
        AND release_key = $previous AND state = 'active'
        AND record_version = $previous_version
        AND record_version > 0 AND record_version < $max_version RETURN AFTER);
    IF $previous_retired = NONE { THROW 'time_previous_authority_conflict'; };
};
COMMIT TRANSACTION;
