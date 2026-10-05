SELECT VALUE candidate
FROM (
  SELECT string::split(collection, '.')[0] AS candidate
  FROM knowledge_collection
  WHERE
    tenant = $tenant
    AND $approvals CONTAINS [collection, document.approval]
    AND $scopes CONTAINSALL document.descriptor.requiredScopes
    AND string::starts_with(string::split(collection, '.')[0], $prefix)
  GROUP BY candidate
  ORDER BY candidate
  LIMIT 101
  TIMEOUT 10s
);
