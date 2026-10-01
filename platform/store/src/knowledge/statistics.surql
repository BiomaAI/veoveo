__URI_SELECTION__
SELECT
    indexedMembers,
    indexedChunks,
    lastObservedAt,
    IF lastModifiedAt = time::minimum { NONE } ELSE { lastModifiedAt } AS lastModifiedAt,
    IF nextExpiry = time::maximum { NONE } ELSE { nextExpiry } AS nextExpiry
FROM (
    SELECT
        count(ordinal = 0) AS indexedMembers,
        count() AS indexedChunks,
        time::max(<datetime> observation.observedAt) AS lastObservedAt,
        time::max(<datetime> (observation.modifiedAt ?? time::minimum)) AS lastModifiedAt,
        time::min(array::min([
            member.valid_until ?? time::maximum,
            admission.expires_at ?? time::maximum,
            array::min(admission.grants[
                WHERE subject IN $subjects AND expires_at > time::now()
            ].expires_at) ?? time::maximum,
            IF collection.document.descriptor.changeSignal != 'immutable' {
                member.sync.coordinator.expires_at
            } ELSE {
                time::maximum
            }
        ])) AS nextExpiry
    FROM __TABLE__
    WHERE __ADMISSION__
    GROUP ALL
    TIMEOUT 10s
);
