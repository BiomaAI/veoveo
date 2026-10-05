BEGIN TRANSACTION;
-- Profile selectors are bound data. Templates consume the first next delimiter;
-- later delimiters cannot rescue an empty variable or an unmatched suffix.
LET $resource_selected = |$collection: string, $uri: string, $selections: array| {
    RETURN $selections.any(|$selection| {
        IF $selection.collection_id != $collection
            OR string::starts_with($uri, string::concat($selection.scheme, '://')) = false {
            RETURN false;
        };
        RETURN $selection.selectors.any(|$selector| {
            IF $selector.kind = 'scheme' {
                RETURN string::starts_with($uri, string::concat($selector.scheme, '://'));
            };
            IF $selector.kind = 'prefix' {
                RETURN string::starts_with($uri, $selector.prefix);
            };
            IF $selector.kind != 'template' { RETURN false; };
            LET $remaining = $selector.literals.fold($uri, |$rest, $literal, $index| {
                IF $rest = NONE { RETURN NONE; };
                IF $index = 0 {
                    RETURN IF string::starts_with($rest, $literal) {
                        string::slice($rest, string::len($literal), string::len($rest))
                    } ELSE { NONE };
                };
                IF $literal = '' { RETURN IF $rest != '' { '' } ELSE { NONE }; };
                LET $parts = string::split($rest, $literal);
                IF array::len($parts) < 2 OR $parts[0] = '' { RETURN NONE; };
                RETURN string::slice($rest, string::len($parts[0]) + string::len($literal), string::len($rest));
            });
            RETURN $remaining = '';
        });
    });
};

LET $statistics = SELECT
    indexed_members,
    indexed_chunks,
    last_observed_at,
    IF last_modified_at = time::minimum { NONE } ELSE { last_modified_at } AS last_modified_at,
    IF next_expiry = time::maximum { NONE } ELSE { next_expiry } AS next_expiry
FROM (
    SELECT
        count(ordinal = 0) AS indexed_members,
        count() AS indexed_chunks,
        time::max(<datetime> observation.observedAt) AS last_observed_at,
        time::max(<datetime> (observation.modifiedAt ?? time::minimum)) AS last_modified_at,
        time::min(array::min([
            member.valid_until ?? time::maximum,
            admission.expires_at ?? time::maximum,
            array::min(admission.grants[
                WHERE subject IN $subjects AND expires_at > time::now()
            ].expires_at) ?? time::maximum,
            IF collection.change_signal != 'immutable' {
                member.sync.coordinator.expires_at
            } ELSE {
                time::maximum
            }
        ])) AS next_expiry
    FROM type::table($chunk_table)
    WHERE   tenant = $tenant
  AND $generation.tenant = $tenant
  AND $generation.state = 'active'
  AND $active.generation = $generation
  AND $resource_selected(collection_id, uri, $resource_selections)
  AND collection.approved = true
  AND $scopes CONTAINSALL collection.required_scopes
  AND collection.tenant = $tenant
  AND collection.collection = collection_id
  AND collection.revision = approval_revision
  AND member.sync.ready = true
  AND member.source_epoch = member.sync.epoch
  AND member.sync.owner_epoch = member.sync.coordinator.epoch
  AND (collection.change_signal = 'immutable' OR member.sync.coordinator.expires_at > time::now())
  AND (member.valid_until = NONE OR member.valid_until > time::now())
  AND member.stale = false
  AND member.deleted = false
  AND member.generation = $generation
  AND member.tenant = $tenant
  AND member.uri = uri
  AND $clearance CONTAINSALL admission.labels
  AND (admission.expires_at = NONE OR admission.expires_at > time::now())
  AND (
    admission.required_context = NONE
    OR admission.required_context = $active_context
  )
  AND (
    admission.required_profile = NONE
    OR admission.required_profile = $profile
  )
  AND (
    admission.tenant_read = true
    OR (
      admission.context_read = true
      AND admission.work_context IN $contexts
    )
    OR (
      admission.selected_context_read = true
      AND admission.work_context = $active_context
      AND admission.work_context IN $contexts
    )
    OR array::len(admission.grants[WHERE subject IN $subjects
      AND (expires_at = NONE OR expires_at > time::now())]) > 0
  )

    GROUP ALL
    TIMEOUT 10s
);

RETURN { rows: $statistics, registration: (SELECT * FROM ONLY $registration WHERE tenant = $tenant AND approved = true AND $scopes CONTAINSALL required_scopes) };
COMMIT TRANSACTION;
