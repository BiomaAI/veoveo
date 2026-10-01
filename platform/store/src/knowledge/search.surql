BEGIN TRANSACTION;
LET $lexical = (
    SELECT id, collection_id, uri, ordinal, search::score(0) AS score
    FROM __TABLE__
    WHERE __ADMISSION__
      AND (array::len($entity_kinds) = 0 OR collection.document.descriptor.entityKind IN $entity_kinds)
      AND text @0@ $query
    ORDER BY score DESC, collection_id ASC, uri ASC, ordinal ASC
    LIMIT $candidates TIMEOUT 10s
);
LET $semantic = (
    SELECT id, collection_id, uri, ordinal, vector::distance::knn() AS distance
    FROM __TABLE__
    WHERE __ADMISSION__
      AND (array::len($entity_kinds) = 0 OR collection.document.descriptor.entityKind IN $entity_kinds)
      AND embedding <|__CANDIDATES__,__EF__|> $vector
    ORDER BY distance ASC, collection_id ASC, uri ASC, ordinal ASC
    LIMIT $candidates TIMEOUT 10s
);
LET $fused = search::rrf([$lexical, $semantic], $candidates * 2, 60);
LET $members = (SELECT collection_id, uri FROM $fused GROUP BY collection_id, uri);
LET $best = $members.map(|$member| $fused[WHERE collection_id = $member.collection_id AND uri = $member.uri][0]);
LET $ranked = (
    SELECT *, id.tenant AS tenant, id.text AS text, id.title AS title, id.admission AS admission,
        id.observation AS observation, id.collection.document.descriptor AS descriptor
    FROM $best
    ORDER BY rrf_score DESC, collection_id ASC, uri ASC
    LIMIT $limit
);
RETURN { rows: $ranked, lexical_count: array::len($lexical), semantic_count: array::len($semantic) };
COMMIT TRANSACTION;
