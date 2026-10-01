SELECT VALUE document FROM knowledge_collection
WHERE tenant = $tenant
    AND enumeration_root = $root
    AND approved = true
    AND $approvals CONTAINS [collection, document.approval]
    AND $scopes CONTAINSALL document.descriptor.requiredScopes
LIMIT 2
TIMEOUT 10s;
