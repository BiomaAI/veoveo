SELECT VALUE candidate
FROM (
  SELECT collection AS candidate
  FROM knowledge_collection
  WHERE
    tenant = $tenant
    AND $approvals CONTAINS [collection, document.approval]
    AND $scopes CONTAINSALL document.descriptor.requiredScopes
    AND string::starts_with(collection, $prefix)
  GROUP BY candidate
  ORDER BY candidate
  LIMIT 101
  TIMEOUT 10s
);
