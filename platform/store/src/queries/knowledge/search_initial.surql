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

LET $lexical = (
    SELECT id, collection_id, uri, ordinal, search::score(0) AS score
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

      AND (array::len($entity_kinds) = 0 OR collection.entity_kind IN $entity_kinds)
      AND text @0@ $query
    ORDER BY score DESC, collection_id ASC, uri ASC, ordinal ASC
    LIMIT $candidates TIMEOUT 10s
);
LET $approximate = (
    SELECT id, collection_id, uri, ordinal, vector::distance::knn() AS distance
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

      AND (array::len($entity_kinds) = 0 OR collection.entity_kind IN $entity_kinds)
      AND embedding <|128,256|> $vector
    ORDER BY distance ASC, collection_id ASC, uri ASC, ordinal ASC
    LIMIT $candidates TIMEOUT 10s
);
-- A short ANN page is not proof that the admitted set is exhausted. Equal
-- vectors and selective predicates can leave the HNSW graph with fewer hits.
-- Recover the candidate window with an exact SQL scan under the same admission.
LET $semantic = IF array::len($approximate) < $candidates {
    SELECT id, collection_id, uri, ordinal,
        1 - vector::similarity::cosine(embedding, $vector) AS distance
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

      AND (array::len($entity_kinds) = 0 OR collection.entity_kind IN $entity_kinds)
    ORDER BY distance ASC, collection_id ASC, uri ASC, ordinal ASC
    LIMIT $candidates TIMEOUT 10s
} ELSE { $approximate };
LET $fused = search::rrf([$lexical, $semantic], $candidates * 2, 60);
LET $members = (SELECT collection_id, uri FROM $fused GROUP BY collection_id, uri);
LET $best = $members.map(|$member| $fused[WHERE collection_id = $member.collection_id AND uri = $member.uri][0]);
LET $ranked = (
    SELECT *, id.tenant AS tenant, id.text AS text, id.title AS title, id.admission AS admission,
        id.observation AS observation, id.collection.* AS registration
    FROM $best
    ORDER BY rrf_score DESC, collection_id ASC, uri ASC
    LIMIT $limit
);
RETURN { rows: $ranked, lexical_count: array::len($lexical), semantic_count: array::len($semantic) };
COMMIT TRANSACTION;
