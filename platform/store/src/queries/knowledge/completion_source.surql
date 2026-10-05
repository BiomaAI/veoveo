SELECT candidate, registrations
FROM (
  SELECT string::split(collection, '.')[0] AS candidate, array::group({id: id, tenant: tenant, collection: collection, enumeration_root: enumeration_root, revision: revision, approved: approved, approval: approval, required_scopes: required_scopes, change_signal: change_signal, entity_kind: entity_kind, document: document}) AS registrations
  FROM knowledge_collection
  WHERE
    tenant = $tenant
    AND $approvals CONTAINS [collection, approval]
    AND $scopes CONTAINSALL required_scopes
    AND string::starts_with(string::split(collection, '.')[0], $prefix)
  GROUP BY candidate
  ORDER BY candidate
  LIMIT 101
  TIMEOUT 10s
);
