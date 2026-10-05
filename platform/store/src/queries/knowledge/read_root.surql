SELECT * FROM knowledge_collection
WHERE tenant = $tenant
    AND enumeration_root = $root
    AND approved = true
    AND $approvals CONTAINS [collection, approval]
    AND $scopes CONTAINSALL required_scopes
LIMIT 2
TIMEOUT 10s;
