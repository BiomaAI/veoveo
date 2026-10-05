UPDATE $record MERGE { status: $status, phase: $phase, staged_release_key: $staged, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() }
WHERE tenant = $tenant AND record_version = $expected
RETURN AFTER;
