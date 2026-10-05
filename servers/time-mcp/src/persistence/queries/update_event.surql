UPDATE $record MERGE { state: $state, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() }
WHERE tenant = $tenant AND owner = $owner AND record_version = $expected
RETURN AFTER;
