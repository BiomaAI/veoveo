UPDATE $record MERGE { owner: $owner, maximum_error_nanoseconds: $maximum_error, maximum_stratum: $maximum_stratum, minimum_source_diversity: $minimum_diversity, maximum_holdover_seconds: $maximum_holdover, record_version: $next, updated_at: time::now() }
WHERE tenant = $tenant AND record_version = $expected
RETURN AFTER;
