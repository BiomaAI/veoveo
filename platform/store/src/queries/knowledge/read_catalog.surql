SELECT * FROM knowledge_collection
WHERE tenant = $tenant
    AND $approvals CONTAINS [collection, approval]
    AND $scopes CONTAINSALL required_scopes
    AND ($selected = NONE OR $selected CONTAINS collection)
ORDER BY collection
LIMIT 1024
TIMEOUT 10s;
