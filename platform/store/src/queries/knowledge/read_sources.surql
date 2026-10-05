SELECT string::split(collection, '.')[0] AS server,
    array::group(document) AS registrations
FROM knowledge_collection
WHERE tenant = $tenant
    AND $approvals CONTAINS [collection, document.approval]
    AND $scopes CONTAINSALL document.descriptor.requiredScopes
    AND ($after = NONE OR string::split(collection, '.')[0] > $after)
GROUP BY server
ORDER BY server
LIMIT 101
TIMEOUT 10s;
