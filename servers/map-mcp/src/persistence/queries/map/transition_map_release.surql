UPDATE $record MERGE { state: $state, canonical_json: $canonical_json, record_version: $next_version, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;
