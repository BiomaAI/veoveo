SELECT VALUE approval FROM knowledge_collection
WHERE tenant = $tenant AND collection = $collection
LIMIT 2
TIMEOUT 10s;
