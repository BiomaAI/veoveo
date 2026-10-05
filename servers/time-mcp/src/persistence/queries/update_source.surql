UPDATE $record MERGE { name: $name, dataset_kind: $dataset_kind, source_url: $source_url, expected_content_type: $expected_content_type, enabled: $enabled, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() }
WHERE tenant = $tenant AND record_version = $expected
RETURN AFTER;
