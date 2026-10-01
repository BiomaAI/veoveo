SELECT collection, document FROM knowledge_collection
WHERE tenant = $tenant
    AND $approvals CONTAINS [collection, document.approval]
    AND $scopes CONTAINSALL document.descriptor.requiredScopes
    AND ($selected = NONE OR $selected CONTAINS collection)
ORDER BY collection
LIMIT 1024
TIMEOUT 10s;
