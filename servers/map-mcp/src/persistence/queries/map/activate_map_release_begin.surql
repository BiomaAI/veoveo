BEGIN TRANSACTION;
LET $release_updated = (UPDATE ONLY $release MERGE { state: 'active', canonical_json: $canonical_json, record_version: $next_release_version, updated_at: time::now() } WHERE tenant = $tenant AND dataset_key = $dataset_key AND record_version = $expected_release_version RETURN AFTER);
IF $release_updated = NONE { THROW 'map_dataset_release_conflict'; };
